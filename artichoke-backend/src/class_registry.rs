use std::any::Any;
use std::convert::TryFrom;

use crate::class;
use crate::exception::Exception;
use crate::ffi::InterpreterExtractError;
use crate::sys;
use crate::types::Int;
use crate::value::Value;
use crate::Artichoke;

pub trait ClassRegistry {
    fn def_class<T>(&mut self, spec: class::Spec) -> Result<(), Exception>
    where
        T: Any;

    fn class_spec<T>(&self) -> Result<Option<&class::Spec>, Exception>
    where
        T: Any;

    fn is_class_defined<T>(&self) -> bool
    where
        T: Any,
    {
        if let Ok(Some(_)) = self.class_spec::<T>() {
            true
        } else {
            false
        }
    }

    fn class_of<T>(&mut self) -> Result<Option<Value>, Exception>
    where
        T: Any;

    fn new_instance<T>(&mut self, args: &[Value]) -> Result<Option<Value>, Exception>
    where
        T: Any;
}

impl ClassRegistry for Artichoke {
    /// Create a class definition bound to a Rust type `T`.
    ///
    /// Class definitions have the same lifetime as the
    /// [`State`](crate::state::State) because the class def owns the
    /// `mrb_data_type` for the type, which must be long-lived.
    fn def_class<T>(&mut self, spec: class::Spec) -> Result<(), Exception>
    where
        T: Any,
    {
        let state = self.state.as_mut().ok_or(InterpreterExtractError)?;
        state.classes.insert::<T>(Box::new(spec));
        Ok(())
    }

    /// Retrieve a class definition from the state bound to Rust type `T`.
    ///
    /// This function returns `None` if type `T` has not had a class spec
    /// registered for it using [`ClassRegistry::def_class`].
    fn class_spec<T>(&self) -> Result<Option<&class::Spec>, Exception>
    where
        T: Any,
    {
        let state = self.state.as_ref().ok_or(InterpreterExtractError)?;
        let spec = state.classes.get::<T>();
        Ok(spec)
    }

    fn class_of<T>(&mut self) -> Result<Option<Value>, Exception>
    where
        T: Any,
    {
        let state = self.state.as_ref().ok_or(InterpreterExtractError)?;
        let spec = state.classes.get::<T>();
        let rclass = if let Some(spec) = spec {
            spec.rclass()
        } else {
            return Ok(None);
        };
        let value_class = unsafe {
            self.with_ffi_boundary(|mrb| {
                if let Some(mut rclass) = rclass.resolve(mrb) {
                    let value_class = sys::mrb_sys_class_value(rclass.as_mut());
                    Some(Value::from(value_class))
                } else {
                    None
                }
            })?
        };
        Ok(value_class)
    }

    fn new_instance<T>(&mut self, args: &[Value]) -> Result<Option<Value>, Exception>
    where
        T: Any,
    {
        let state = self.state.as_ref().ok_or(InterpreterExtractError)?;
        let spec = state.classes.get::<T>();
        let rclass = if let Some(spec) = spec {
            spec.rclass()
        } else {
            return Ok(None);
        };
        let args = args.iter().map(Value::inner).collect::<Vec<_>>();
        let arglen = if let Ok(len) = Int::try_from(args.len()) {
            len
        } else {
            return Ok(None);
        };
        let instance = unsafe {
            self.with_ffi_boundary(|mrb| {
                if let Some(mut rclass) = rclass.resolve(mrb) {
                    let value = sys::mrb_obj_new(mrb, rclass.as_mut(), arglen, args.as_ptr());
                    Some(Value::from(value))
                } else {
                    None
                }
            })?
        };

        Ok(instance)
    }
}
