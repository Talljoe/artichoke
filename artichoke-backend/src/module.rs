use std::borrow::Cow;
use std::collections::HashSet;
use std::convert::AsRef;
use std::ffi::{c_void, CStr, CString};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ptr::NonNull;

use crate::core::Intern;
use crate::def::{ConstantNameError, EnclosingRubyScope, Method, NotDefinedError};
use crate::exception::Exception;
use crate::intern::Symbol;
use crate::method;
use crate::sys;
use crate::Artichoke;

mod registry;

pub use registry::Registry;

#[derive(Debug)]
pub struct Builder<'a> {
    interp: &'a mut Artichoke,
    spec: &'a Spec,
    methods: HashSet<method::Spec>,
}

impl<'a> Builder<'a> {
    #[must_use]
    pub fn for_spec(interp: &'a mut Artichoke, spec: &'a Spec) -> Self {
        Self {
            interp,
            spec,
            methods: HashSet::default(),
        }
    }

    pub fn add_method<T>(
        mut self,
        name: T,
        method: Method,
        args: sys::mrb_aspec,
    ) -> Result<Self, ConstantNameError>
    where
        T: Into<Cow<'static, str>>,
    {
        let spec = method::Spec::new(method::Type::Instance, name.into(), method, args)?;
        self.methods.insert(spec);
        Ok(self)
    }

    pub fn add_self_method<T>(
        mut self,
        name: T,
        method: Method,
        args: sys::mrb_aspec,
    ) -> Result<Self, ConstantNameError>
    where
        T: Into<Cow<'static, str>>,
    {
        let spec = method::Spec::new(method::Type::Class, name.into(), method, args)?;
        self.methods.insert(spec);
        Ok(self)
    }

    pub fn add_module_method<T>(
        mut self,
        name: T,
        method: Method,
        args: sys::mrb_aspec,
    ) -> Result<Self, ConstantNameError>
    where
        T: Into<Cow<'static, str>>,
    {
        let spec = method::Spec::new(method::Type::Module, name.into(), method, args)?;
        self.methods.insert(spec);
        Ok(self)
    }

    pub fn define(self) -> Result<(), NotDefinedError> {
        let name = self.spec.name_c_str().as_ptr();

        let rclass = self.spec.rclass();
        let rclass = unsafe { self.interp.with_ffi_boundary(|mrb| rclass.resolve(mrb)) };

        let mut rclass = if let Ok(Some(rclass)) = rclass {
            rclass
        } else if let Some(enclosing_scope) = self.spec.enclosing_scope() {
            let scope = unsafe {
                self.interp
                    .with_ffi_boundary(|mrb| enclosing_scope.rclass(mrb))
            };
            if let Ok(Some(mut scope)) = scope {
                let rclass = unsafe {
                    self.interp.with_ffi_boundary(|mrb| {
                        sys::mrb_define_module_under(mrb, scope.as_mut(), name)
                    })
                };
                let rclass = rclass.map_err(|_| NotDefinedError::module(self.spec.name()))?;
                NonNull::new(rclass).ok_or_else(|| NotDefinedError::module(self.spec.name()))?
            } else {
                return Err(NotDefinedError::enclosing_scope(
                    enclosing_scope.fqname().into_owned(),
                ));
            }
        } else {
            let rclass = unsafe {
                self.interp
                    .with_ffi_boundary(|mrb| sys::mrb_define_module(mrb, name))
            };
            let rclass = rclass.map_err(|_| NotDefinedError::module(self.spec.name()))?;
            NonNull::new(rclass).ok_or_else(|| NotDefinedError::module(self.spec.name()))?
        };

        for method in self.methods {
            unsafe {
                method.define(self.interp, rclass.as_mut())?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rclass {
    sym: Symbol,
    name: CString,
    enclosing_scope: Option<Box<EnclosingRubyScope>>,
}

impl Rclass {
    #[must_use]
    pub fn new(
        sym: Symbol,
        name: CString,
        enclosing_scope: Option<Box<EnclosingRubyScope>>,
    ) -> Self {
        Self {
            sym,
            name,
            enclosing_scope,
        }
    }

    /// Resolve a type's [`sys::RClass`] using its enclosing scope and name.
    ///
    /// # Safety
    ///
    /// This function must be called within an [`Artichoke::with_ffi_boundary`]
    /// closure because the FFI APIs called in this function may require access
    /// to the Artichoke [`State](crate::state::State).
    pub unsafe fn resolve(&self, mrb: *mut sys::mrb_state) -> Option<NonNull<sys::RClass>> {
        let module_name = self.name.as_ptr();
        if let Some(ref scope) = self.enclosing_scope {
            // Short circuit if enclosing scope does not exist.
            let mut scope = scope.rclass(mrb)?;
            let is_defined_under = sys::mrb_const_defined_at(
                mrb,
                sys::mrb_sys_obj_value(scope.cast::<c_void>().as_mut()),
                self.sym.into(),
            );
            if is_defined_under == 0 {
                // Enclosing scope exists.
                // Module is not defined under the enclosing scope.
                None
            } else {
                // Enclosing scope exists.
                // Module is defined under the enclosing scope.
                let module = sys::mrb_module_get_under(mrb, scope.as_mut(), module_name);
                NonNull::new(module)
            }
        } else {
            let is_defined = sys::mrb_const_defined_at(
                mrb,
                sys::mrb_sys_obj_value((*mrb).object_class as *mut c_void),
                self.sym.into(),
            );
            if is_defined == 0 {
                // Class does not exist in root scope.
                None
            } else {
                // Module exists in root scope.
                let module = sys::mrb_module_get(mrb, module_name);
                NonNull::new(module)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Spec {
    name: Cow<'static, str>,
    sym: Symbol,
    cstring: CString,
    enclosing_scope: Option<Box<EnclosingRubyScope>>,
}

impl Spec {
    pub fn new<T>(
        interp: &mut Artichoke,
        name: T,
        enclosing_scope: Option<EnclosingRubyScope>,
    ) -> Result<Self, Exception>
    where
        T: Into<Cow<'static, str>>,
    {
        let name = name.into();
        if let Ok(cstring) = CString::new(name.as_ref()) {
            let sym = match name {
                Cow::Borrowed(name) => interp.intern_string(name)?,
                Cow::Owned(ref name) => interp.intern_string(name.clone())?,
            };
            Ok(Self {
                name,
                cstring,
                sym,
                enclosing_scope: enclosing_scope.map(Box::new),
            })
        } else {
            Err(ConstantNameError::from(name).into())
        }
    }

    #[must_use]
    pub fn name(&self) -> Cow<'static, str> {
        match &self.name {
            Cow::Borrowed(name) => Cow::Borrowed(name),
            Cow::Owned(name) => name.clone().into(),
        }
    }

    #[must_use]
    pub fn name_c_str(&self) -> &CStr {
        self.cstring.as_c_str()
    }

    #[must_use]
    pub fn enclosing_scope(&self) -> Option<&EnclosingRubyScope> {
        self.enclosing_scope.as_deref()
    }

    #[must_use]
    pub fn fqname(&self) -> Cow<'_, str> {
        if let Some(scope) = self.enclosing_scope() {
            let mut fqname = String::from(scope.fqname());
            fqname.push_str("::");
            fqname.push_str(self.name.as_ref());
            fqname.into()
        } else {
            self.name.as_ref().into()
        }
    }

    #[must_use]
    pub fn rclass(&self) -> Rclass {
        Rclass::new(self.sym, self.cstring.clone(), self.enclosing_scope.clone())
    }
}

impl fmt::Display for Spec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "artichoke module spec -- {}", self.fqname())
    }
}

impl Hash for Spec {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name().hash(state);
        self.enclosing_scope().hash(state);
    }
}

impl Eq for Spec {}

impl PartialEq for Spec {
    fn eq(&self, other: &Self) -> bool {
        self.fqname() == other.fqname()
    }
}

#[cfg(test)]
mod tests {
    use crate::module::Spec;
    use crate::test::prelude::*;

    #[test]
    fn rclass_for_undef_root_module() {
        let mut interp = crate::interpreter().unwrap();
        let spec = Spec::new(&mut interp, "Foo", None).unwrap();
        let rclass = unsafe { interp.with_ffi_boundary(|mrb| spec.rclass().resolve(mrb)) }.unwrap();
        assert!(rclass.is_none());
    }

    #[test]
    fn rclass_for_undef_nested_module() {
        let mut interp = crate::interpreter().unwrap();
        let scope = Spec::new(&mut interp, "Kernel", None).unwrap();
        let scope = EnclosingRubyScope::module(&scope);
        let spec = Spec::new(&mut interp, "Foo", Some(scope)).unwrap();
        let rclass = unsafe { interp.with_ffi_boundary(|mrb| spec.rclass().resolve(mrb)) }.unwrap();
        assert!(rclass.is_none());
    }

    #[test]
    fn rclass_for_root_module() {
        let mut interp = crate::interpreter().unwrap();
        let spec = Spec::new(&mut interp, "Kernel", None).unwrap();
        let rclass = unsafe { interp.with_ffi_boundary(|mrb| spec.rclass().resolve(mrb)) }.unwrap();
        assert!(rclass.is_some());
    }

    #[test]
    fn rclass_for_nested_module() {
        let mut interp = crate::interpreter().unwrap();
        let _ = interp.eval(b"module Foo; module Bar; end; end").unwrap();
        let scope = Spec::new(&mut interp, "Foo", None).unwrap();
        let scope = EnclosingRubyScope::module(&scope);
        let spec = Spec::new(&mut interp, "Bar", Some(scope)).unwrap();
        let rclass = unsafe { interp.with_ffi_boundary(|mrb| spec.rclass().resolve(mrb)) }.unwrap();
        assert!(rclass.is_some());
    }

    #[test]
    fn rclass_for_nested_module_under_class() {
        let mut interp = crate::interpreter().unwrap();
        let _ = interp.eval(b"class Foo; module Bar; end; end").unwrap();
        let scope = class::Spec::new("Foo", None, None).unwrap();
        let scope = EnclosingRubyScope::class(&scope);
        let spec = Spec::new(&mut interp, "Bar", Some(scope)).unwrap();
        let rclass = unsafe { interp.with_ffi_boundary(|mrb| spec.rclass().resolve(mrb)) }.unwrap();
        assert!(rclass.is_some());
    }
}
