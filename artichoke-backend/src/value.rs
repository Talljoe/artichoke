use std::borrow::Cow;
use std::convert::TryFrom;
use std::error;
use std::fmt;
use std::mem;
use std::ptr;

use crate::class_registry::ClassRegistry;
use crate::convert::BoxUnboxVmValue;
use crate::core::{Convert, ConvertMut, Intern, TryConvert, Value as ValueCore};
use crate::exception::{Exception, RubyException};
use crate::exception_handler;
use crate::extn::core::exception::{ArgumentError, Fatal, TypeError};
use crate::extn::core::symbol::Symbol;
use crate::gc::MrbGarbageCollection;
use crate::sys::{self, protect};
use crate::types::{self, Int, Ruby};
use crate::Artichoke;

/// Max argument count for function calls including initialize and yield.
pub const MRB_FUNCALL_ARGC_MAX: usize = 16;

/// Boxed Ruby value in the [`Artichoke`] interpreter.
#[derive(Default, Debug, Clone, Copy)]
pub struct Value(sys::mrb_value);

impl From<sys::mrb_value> for Value {
    /// Construct a new [`Value`] from a [`sys::mrb_value`].
    fn from(value: sys::mrb_value) -> Self {
        Self(value)
    }
}

impl From<Option<sys::mrb_value>> for Value {
    fn from(value: Option<sys::mrb_value>) -> Self {
        if let Some(value) = value {
            Self::from(value)
        } else {
            Self::nil()
        }
    }
}

impl From<Option<Value>> for Value {
    fn from(value: Option<Value>) -> Self {
        value.unwrap_or_else(Value::nil)
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        let this = unsafe { sys::mrb_sys_basic_ptr(self.inner()) };
        let other = unsafe { sys::mrb_sys_basic_ptr(other.inner()) };
        ptr::eq(this, other)
    }
}

impl Value {
    /// Create a new, empty Ruby value.
    ///
    /// Alias for `Value::default`.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a `nil` Ruby Value.
    #[inline]
    #[must_use]
    pub fn nil() -> Self {
        Self::default()
    }

    /// The [`sys::mrb_value`] that this [`Value`] wraps.
    // TODO(GH-251): make `Value::inner` pub(crate).
    #[inline]
    #[must_use]
    pub fn inner(&self) -> sys::mrb_value {
        self.0
    }

    /// Return this values [Rust-mapped type tag](Ruby).
    #[inline]
    #[must_use]
    pub fn ruby_type(&self) -> Ruby {
        types::ruby_from_mrb_value(self.inner())
    }

    #[must_use]
    pub fn pretty_name<'a>(&self, interp: &mut Artichoke) -> &'a str {
        match self.try_into(interp) {
            Ok(Some(true)) => "true",
            Ok(Some(false)) => "false",
            Ok(None) => "nil",
            Err(_) => {
                if let Ruby::Data | Ruby::Object = self.ruby_type() {
                    self.funcall(interp, "class", &[], None)
                        .and_then(|class| class.funcall(interp, "name", &[], None))
                        .and_then(|class| class.try_into_mut(interp))
                        .unwrap_or_default()
                } else {
                    self.ruby_type().class_name()
                }
            }
        }
    }

    /// Whether a value is an interpreter-only variant not exposed to Ruby.
    ///
    /// Some type tags like [`MRB_TT_UNDEF`](sys::mrb_vtype::MRB_TT_UNDEF) are
    /// internal to the mruby VM and manipulating them with the [`sys`] API is
    /// unspecified and may result in a segfault.
    ///
    /// After extracting a [`sys::mrb_value`] from the interpreter, check to see
    /// if the value is [unreachable](Ruby::Unreachable) a [`Fatal`] exception.
    ///
    /// See: [mruby#4460](https://github.com/mruby/mruby/issues/4460).
    #[must_use]
    #[inline]
    pub fn is_unreachable(&self) -> bool {
        matches!(self.ruby_type(), Ruby::Unreachable)
    }

    /// Return whether this object is unreachable by any GC roots.
    #[must_use]
    pub fn is_dead(&self, interp: &mut Artichoke) -> bool {
        let value = self.inner();
        let is_dead =
            unsafe { interp.with_ffi_boundary(|mrb| sys::mrb_sys_value_is_dead(mrb, value)) };
        is_dead.unwrap_or_default()
    }

    pub fn is_range(
        &self,
        interp: &mut Artichoke,
        len: Int,
    ) -> Result<Option<protect::Range>, Exception> {
        let mut arena = interp.create_arena_savepoint();
        let result = unsafe {
            arena
                .interp()
                .with_ffi_boundary(|mrb| protect::is_range(mrb, self.inner(), len))?
        };
        match result {
            Ok(range) => Ok(range),
            Err(exception) => {
                let exception = Self::from(exception);
                Err(exception_handler::last_error(&mut arena, exception)?)
            }
        }
    }

    pub fn implicitly_convert_to_int(&self, interp: &mut Artichoke) -> Result<Int, TypeError> {
        let int = if let Ok(int) = self.try_into::<Option<Int>>(interp) {
            if let Some(int) = int {
                int
            } else {
                return Err(TypeError::from(
                    "no implicit conversion from nil to integer",
                ));
            }
        } else if let Ok(true) = self.respond_to(interp, "to_int") {
            if let Ok(maybe) = self.funcall(interp, "to_int", &[], None) {
                if let Ok(int) = maybe.try_into::<Int>(interp) {
                    int
                } else {
                    let mut message = String::from("can't convert ");
                    message.push_str(self.pretty_name(interp));
                    message.push_str(" to Integer (");
                    message.push_str(self.pretty_name(interp));
                    message.push_str("#to_int gives ");
                    message.push_str(maybe.pretty_name(interp));
                    message.push(')');
                    return Err(TypeError::from(message));
                }
            } else {
                let mut message = String::from("no implicit conversion of ");
                message.push_str(self.pretty_name(interp));
                message.push_str(" into Integer");
                return Err(TypeError::from(message));
            }
        } else {
            let mut message = String::from("no implicit conversion of ");
            message.push_str(self.pretty_name(interp));
            message.push_str(" into Integer");
            return Err(TypeError::from(message));
        };
        Ok(int)
    }

    pub fn implicitly_convert_to_string(&self, interp: &mut Artichoke) -> Result<&[u8], TypeError> {
        let string = if let Ok(string) = self.try_into_mut::<&[u8]>(interp) {
            string
        } else if let Ruby::Symbol = self.ruby_type() {
            let mut value = *self;
            // Infallible because of Symbol ruby type
            let sym = unsafe { Symbol::unbox_from_value(&mut value, interp).unwrap() };
            let bytes = sym.bytes(interp);
            // Safety:
            //
            // Symbols are valid for the lifetime of the interpreter, which is a
            // longer lifetime than `self`.
            //
            // This transmute shrinks the lifetime of the interned bytes to the
            // lifetime of this `Value`.
            unsafe { mem::transmute(bytes) }
        } else if let Ok(true) = self.respond_to(interp, "to_str") {
            if let Ok(maybe) = self.funcall(interp, "to_str", &[], None) {
                if let Ok(string) = maybe.try_into_mut::<&[u8]>(interp) {
                    string
                } else {
                    let mut message = String::from("can't convert ");
                    message.push_str(self.pretty_name(interp));
                    message.push_str(" to String (");
                    message.push_str(self.pretty_name(interp));
                    message.push_str("#to_str gives ");
                    message.push_str(maybe.pretty_name(interp));
                    message.push(')');
                    return Err(TypeError::from(message));
                }
            } else {
                let mut message = String::from("no implicit conversion of ");
                message.push_str(self.pretty_name(interp));
                message.push_str(" into String");
                return Err(TypeError::from(message));
            }
        } else {
            let mut message = String::from("no implicit conversion of ");
            message.push_str(self.pretty_name(interp));
            message.push_str(" into String");
            return Err(TypeError::from(message));
        };
        Ok(string)
    }

    #[inline]
    pub fn implicitly_convert_to_nilable_string(
        &self,
        interp: &mut Artichoke,
    ) -> Result<Option<&[u8]>, TypeError> {
        if self.is_nil() {
            Ok(None)
        } else {
            self.implicitly_convert_to_string(interp).map(Some)
        }
    }
}

impl ValueCore for Value {
    type Artichoke = Artichoke;
    type Arg = Self;
    type Value = Self;
    type Block = Self;
    type Error = Exception;

    fn funcall(
        &self,
        interp: &mut Self::Artichoke,
        func: &str,
        args: &[Self::Arg],
        block: Option<Self::Block>,
    ) -> Result<Self::Value, Self::Error> {
        let mut arena = interp.create_arena_savepoint();
        if let Ok(arg_count_error) = ArgCountError::try_from(args) {
            warn!("{}", arg_count_error);
            return Err(arg_count_error.into());
        }
        let args = args.iter().map(Self::inner).collect::<Vec<_>>();
        trace!(
            "Calling {}#{} with {} args{}",
            self.ruby_type(),
            func,
            args.len(),
            if block.is_some() { " and block" } else { "" }
        );
        let func = arena.intern_string(func.to_string())?;
        let result = unsafe {
            arena.with_ffi_boundary(|mrb| {
                protect::funcall(
                    mrb,
                    self.inner(),
                    func.into(),
                    args.as_slice(),
                    block.as_ref().map(Self::inner),
                )
            })?
        };
        match result {
            Ok(value) => {
                let value = Self::from(value);
                if value.is_unreachable() {
                    // Unreachable values are internal to the mruby interpreter
                    // and interacting with them via the C API is unspecified
                    // and may result in a segfault.
                    //
                    // See: https://github.com/mruby/mruby/issues/4460
                    Err(Fatal::from("Unreachable Ruby value").into())
                } else {
                    Ok(value)
                }
            }
            Err(exception) => {
                let exception = Self::from(exception);
                Err(exception_handler::last_error(&mut arena, exception)?)
            }
        }
    }

    fn freeze(&mut self, interp: &mut Self::Artichoke) -> Result<(), Self::Error> {
        let _ = self.funcall(interp, "freeze", &[], None)?;
        Ok(())
    }

    fn is_frozen(&self, interp: &mut Self::Artichoke) -> bool {
        let value = self.inner();
        let is_frozen =
            unsafe { interp.with_ffi_boundary(|mrb| sys::mrb_sys_obj_frozen(mrb, value)) };
        is_frozen.unwrap_or_default()
    }

    fn inspect(&self, interp: &mut Self::Artichoke) -> Vec<u8> {
        if let Ok(display) = self.funcall(interp, "inspect", &[], None) {
            display.try_into_mut(interp).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    fn is_nil(&self) -> bool {
        matches!(self.ruby_type(), Ruby::Nil)
    }

    fn respond_to(&self, interp: &mut Self::Artichoke, method: &str) -> Result<bool, Self::Error> {
        let method = interp.convert_mut(method);
        let respond_to = self.funcall(interp, "respond_to?", &[method], None)?;
        interp.try_convert(respond_to)
    }

    fn to_s(&self, interp: &mut Self::Artichoke) -> Vec<u8> {
        if let Ok(display) = self.funcall(interp, "to_s", &[], None) {
            display.try_into_mut(interp).unwrap_or_default()
        } else {
            Vec::new()
        }
    }
}

impl Convert<Value, Value> for Artichoke {
    fn convert(&self, value: Value) -> Value {
        value
    }
}

impl ConvertMut<Value, Value> for Artichoke {
    fn convert_mut(&mut self, value: Value) -> Value {
        value
    }
}

/// Argument count exceeds maximum allowed by the VM.
#[derive(Default, Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArgCountError {
    /// Number of arguments given.
    pub given: usize,
    /// Maximum number of arguments supported.
    pub max: usize,
}

impl TryFrom<Vec<Value>> for ArgCountError {
    type Error = ();

    fn try_from(args: Vec<Value>) -> Result<Self, Self::Error> {
        if args.len() > MRB_FUNCALL_ARGC_MAX {
            Ok(Self {
                given: args.len(),
                max: MRB_FUNCALL_ARGC_MAX,
            })
        } else {
            Err(())
        }
    }
}

impl TryFrom<Vec<sys::mrb_value>> for ArgCountError {
    type Error = ();

    fn try_from(args: Vec<sys::mrb_value>) -> Result<Self, Self::Error> {
        if args.len() > MRB_FUNCALL_ARGC_MAX {
            Ok(Self {
                given: args.len(),
                max: MRB_FUNCALL_ARGC_MAX,
            })
        } else {
            Err(())
        }
    }
}

impl TryFrom<&[Value]> for ArgCountError {
    type Error = ();

    fn try_from(args: &[Value]) -> Result<Self, Self::Error> {
        if args.len() > MRB_FUNCALL_ARGC_MAX {
            Ok(Self {
                given: args.len(),
                max: MRB_FUNCALL_ARGC_MAX,
            })
        } else {
            Err(())
        }
    }
}

impl TryFrom<&[sys::mrb_value]> for ArgCountError {
    type Error = ();

    fn try_from(args: &[sys::mrb_value]) -> Result<Self, Self::Error> {
        if args.len() > MRB_FUNCALL_ARGC_MAX {
            Ok(Self {
                given: args.len(),
                max: MRB_FUNCALL_ARGC_MAX,
            })
        } else {
            Err(())
        }
    }
}

impl ArgCountError {
    /// Constructs a new, empty `ArgCountError`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            given: 0,
            max: MRB_FUNCALL_ARGC_MAX,
        }
    }
}

impl fmt::Display for ArgCountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Too many arguments for function call: ")?;
        write!(
            f,
            "gave {} arguments, but Artichoke only supports a maximum of {} arguments",
            self.given, self.max
        )
    }
}

impl error::Error for ArgCountError {}

impl RubyException for ArgCountError {
    fn message(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(b"Too many arguments")
    }

    fn name(&self) -> Cow<'_, str> {
        "ArgumentError".into()
    }

    fn vm_backtrace(&self, interp: &mut Artichoke) -> Option<Vec<Vec<u8>>> {
        let _ = interp;
        None
    }

    fn as_mrb_value(&self, interp: &mut Artichoke) -> Option<sys::mrb_value> {
        let message = interp.convert_mut(self.to_string());
        let value = interp
            .new_instance::<ArgumentError>(&[message])
            .ok()
            .flatten()?;
        Some(value.inner())
    }
}

impl From<ArgCountError> for Exception {
    fn from(exception: ArgCountError) -> Self {
        Self::from(Box::<dyn RubyException>::from(exception))
    }
}

impl From<Box<ArgCountError>> for Exception {
    fn from(exception: Box<ArgCountError>) -> Self {
        Self::from(Box::<dyn RubyException>::from(exception))
    }
}

impl From<ArgCountError> for Box<dyn RubyException> {
    fn from(exception: ArgCountError) -> Box<dyn RubyException> {
        Box::new(exception)
    }
}

impl From<Box<ArgCountError>> for Box<dyn RubyException> {
    fn from(exception: Box<ArgCountError>) -> Box<dyn RubyException> {
        exception
    }
}

#[cfg(test)]
mod tests {
    use crate::gc::MrbGarbageCollection;
    use crate::test::prelude::*;

    #[test]
    fn to_s_true() {
        let mut interp = crate::interpreter().unwrap();

        let value = interp.convert(true);
        let string = value.to_s(&mut interp);
        assert_eq!(string, b"true");
    }

    #[test]
    fn inspect_true() {
        let mut interp = crate::interpreter().unwrap();

        let value = interp.convert(true);
        let debug = value.inspect(&mut interp);
        assert_eq!(debug, b"true");
    }

    #[test]
    fn to_s_false() {
        let mut interp = crate::interpreter().unwrap();

        let value = interp.convert(false);
        let string = value.to_s(&mut interp);
        assert_eq!(string, b"false");
    }

    #[test]
    fn inspect_false() {
        let mut interp = crate::interpreter().unwrap();

        let value = interp.convert(false);
        let debug = value.inspect(&mut interp);
        assert_eq!(debug, b"false");
    }

    #[test]
    fn to_s_nil() {
        let mut interp = crate::interpreter().unwrap();

        let value = Value::nil();
        let string = value.to_s(&mut interp);
        assert_eq!(string, b"");
    }

    #[test]
    fn inspect_nil() {
        let mut interp = crate::interpreter().unwrap();

        let value = Value::nil();
        let debug = value.inspect(&mut interp);
        assert_eq!(debug, b"nil");
    }

    #[test]
    fn to_s_fixnum() {
        let mut interp = crate::interpreter().unwrap();

        let value = Convert::<_, Value>::convert(&interp, 255);
        let string = value.to_s(&mut interp);
        assert_eq!(string, b"255");
    }

    #[test]
    fn inspect_fixnum() {
        let mut interp = crate::interpreter().unwrap();

        let value = Convert::<_, Value>::convert(&interp, 255);
        let debug = value.inspect(&mut interp);
        assert_eq!(debug, b"255");
    }

    #[test]
    fn to_s_string() {
        let mut interp = crate::interpreter().unwrap();

        let value = interp.convert_mut("interstate");
        let string = value.to_s(&mut interp);
        assert_eq!(string, b"interstate");
    }

    #[test]
    fn inspect_string() {
        let mut interp = crate::interpreter().unwrap();

        let value = interp.convert_mut("interstate");
        let debug = value.inspect(&mut interp);
        assert_eq!(debug, br#""interstate""#);
    }

    #[test]
    fn to_s_empty_string() {
        let mut interp = crate::interpreter().unwrap();

        let value = interp.convert_mut("");
        let string = value.to_s(&mut interp);
        assert_eq!(string, b"");
    }

    #[test]
    fn inspect_empty_string() {
        let mut interp = crate::interpreter().unwrap();

        let value = interp.convert_mut("");
        let debug = value.inspect(&mut interp);
        assert_eq!(debug, br#""""#);
    }

    #[test]
    fn is_dead() {
        let mut interp = crate::interpreter().unwrap();
        let mut arena = interp.create_arena_savepoint();
        let live = arena.eval(b"'dead'").unwrap();
        assert!(!live.is_dead(&mut arena));
        let dead = live;
        let live = arena.eval(b"'live'").unwrap();
        arena.restore();
        interp.full_gc();
        // unreachable objects are dead after a full garbage collection
        assert!(dead.is_dead(&mut interp));
        // the result of the most recent eval is always live even after a full
        // garbage collection
        assert!(!live.is_dead(&mut interp));
    }

    #[test]
    fn immediate_is_dead() {
        let mut interp = crate::interpreter().unwrap();
        let mut arena = interp.create_arena_savepoint();
        let live = arena.eval(b"27").unwrap();
        assert!(!live.is_dead(&mut arena));
        let immediate = live;
        let live = arena.eval(b"64").unwrap();
        arena.restore();
        interp.full_gc();
        // immediate objects are never dead
        assert!(!immediate.is_dead(&mut interp));
        // the result of the most recent eval is always live even after a full
        // garbage collection
        assert!(!live.is_dead(&mut interp));
        // Fixnums are immediate even if they are created directly without an
        // interpreter.
        let fixnum = Convert::<_, Value>::convert(&interp, 99);
        assert!(!fixnum.is_dead(&mut interp));
    }

    #[test]
    fn funcall() {
        let mut interp = crate::interpreter().unwrap();
        let nil = Value::nil();
        let nil_is_nil = nil
            .funcall(&mut interp, "nil?", &[], None)
            .and_then(|value| value.try_into::<bool>(&interp))
            .unwrap();
        assert!(nil_is_nil);
        let s = interp.convert_mut("foo");
        let string_is_nil = s
            .funcall(&mut interp, "nil?", &[], None)
            .and_then(|value| value.try_into::<bool>(&interp))
            .unwrap();
        assert!(!string_is_nil);
        let delim = interp.convert_mut("");
        let split = s.funcall(&mut interp, "split", &[delim], None).unwrap();
        let split = split.try_into_mut::<Vec<&str>>(&mut interp).unwrap();
        assert_eq!(split, vec!["f", "o", "o"])
    }

    #[test]
    fn funcall_different_types() {
        let mut interp = crate::interpreter().unwrap();
        let nil = Value::nil();
        let s = interp.convert_mut("foo");
        let eql = nil
            .funcall(&mut interp, "==", &[s], None)
            .and_then(|value| value.try_into::<bool>(&interp))
            .unwrap();
        assert!(!eql);
    }

    #[test]
    fn funcall_type_error() {
        let mut interp = crate::interpreter().unwrap();
        let nil = Value::nil();
        let s = interp.convert_mut("foo");
        let err = s
            .funcall(&mut interp, "+", &[nil], None)
            .and_then(|value| value.try_into_mut::<String>(&mut interp))
            .unwrap_err();
        assert_eq!("TypeError", err.name().as_ref());
        assert_eq!(
            &b"nil cannot be converted to String"[..],
            err.message().as_ref()
        );
    }

    #[test]
    fn funcall_method_not_exists() {
        let mut interp = crate::interpreter().unwrap();
        let nil = Value::nil();
        let s = interp.convert_mut("foo");
        let err = nil
            .funcall(&mut interp, "garbage_method_name", &[s], None)
            .unwrap_err();
        assert_eq!("NoMethodError", err.name().as_ref());
        assert_eq!(
            &b"undefined method 'garbage_method_name'"[..],
            err.message().as_ref()
        );
    }
}
