use crate::extn::prelude::*;
use crate::extn::stdlib::securerandom::{self, trampoline};

pub fn init(interp: &mut Artichoke) -> InitializeResult<()> {
    interp.def_file_for_type::<_, SecureRandomFile>("securerandom.rb")?;
    Ok(())
}

#[derive(Debug)]
pub struct SecureRandomFile;

impl File for SecureRandomFile {
    type Artichoke = Artichoke;
    type Error = Exception;

    fn require(interp: &mut Self::Artichoke) -> Result<(), Self::Error> {
        if interp.is_module_defined::<securerandom::SecureRandom>() {
            return Ok(());
        }
        let spec = module::Spec::new(interp, "SecureRandom", None)?;
        module::Builder::for_spec(interp, &spec)
            .add_self_method(
                "alphanumeric",
                artichoke_securerandom_alphanumeric,
                sys::mrb_args_opt(1),
            )?
            .add_self_method(
                "base64",
                artichoke_securerandom_base64,
                sys::mrb_args_opt(1),
            )?
            .add_self_method("hex", artichoke_securerandom_hex, sys::mrb_args_opt(1))?
            .add_self_method(
                "random_bytes",
                artichoke_securerandom_random_bytes,
                sys::mrb_args_opt(1),
            )?
            .add_self_method(
                "random_number",
                artichoke_securerandom_random_number,
                sys::mrb_args_opt(1),
            )?
            .add_self_method("uuid", artichoke_securerandom_uuid, sys::mrb_args_none())?
            .define()?;
        interp.def_module::<securerandom::SecureRandom>(spec)?;

        trace!("Patched SecureRandom onto interpreter");
        Ok(())
    }
}

#[no_mangle]
unsafe extern "C" fn artichoke_securerandom_alphanumeric(
    mrb: *mut sys::mrb_state,
    _slf: sys::mrb_value,
) -> sys::mrb_value {
    let len = mrb_get_args!(mrb, optional = 1);
    let mut interp = unwrap_interpreter!(mrb);
    let mut guard = Guard::new(&mut interp);
    let len = len.map(Value::from).and_then(|len| guard.convert(len));
    let result = trampoline::alphanumeric(&mut guard, len);
    match result {
        Ok(value) => value.inner(),
        Err(exception) => exception::raise(guard, exception),
    }
}

#[no_mangle]
unsafe extern "C" fn artichoke_securerandom_base64(
    mrb: *mut sys::mrb_state,
    _slf: sys::mrb_value,
) -> sys::mrb_value {
    let len = mrb_get_args!(mrb, optional = 1);
    let mut interp = unwrap_interpreter!(mrb);
    let mut guard = Guard::new(&mut interp);
    let len = len.map(Value::from).and_then(|len| guard.convert(len));
    let result = trampoline::base64(&mut guard, len);
    match result {
        Ok(value) => value.inner(),
        Err(exception) => exception::raise(guard, exception),
    }
}

#[no_mangle]
unsafe extern "C" fn artichoke_securerandom_hex(
    mrb: *mut sys::mrb_state,
    _slf: sys::mrb_value,
) -> sys::mrb_value {
    let len = mrb_get_args!(mrb, optional = 1);
    let mut interp = unwrap_interpreter!(mrb);
    let mut guard = Guard::new(&mut interp);
    let len = len.map(Value::from).and_then(|len| guard.convert(len));
    let result = trampoline::hex(&mut guard, len);
    match result {
        Ok(value) => value.inner(),
        Err(exception) => exception::raise(guard, exception),
    }
}

#[no_mangle]
unsafe extern "C" fn artichoke_securerandom_random_bytes(
    mrb: *mut sys::mrb_state,
    _slf: sys::mrb_value,
) -> sys::mrb_value {
    let len = mrb_get_args!(mrb, optional = 1);
    let mut interp = unwrap_interpreter!(mrb);
    let mut guard = Guard::new(&mut interp);
    let len = len.map(Value::from).and_then(|len| guard.convert(len));
    let result = trampoline::random_bytes(&mut guard, len);
    match result {
        Ok(value) => value.inner(),
        Err(exception) => exception::raise(guard, exception),
    }
}

#[no_mangle]
unsafe extern "C" fn artichoke_securerandom_random_number(
    mrb: *mut sys::mrb_state,
    _slf: sys::mrb_value,
) -> sys::mrb_value {
    let max = mrb_get_args!(mrb, optional = 1);
    let mut interp = unwrap_interpreter!(mrb);
    let mut guard = Guard::new(&mut interp);
    let max = max.map(Value::from).and_then(|max| guard.convert(max));
    let result = trampoline::random_number(&mut guard, max);
    match result {
        Ok(value) => value.inner(),
        Err(exception) => exception::raise(guard, exception),
    }
}

#[no_mangle]
unsafe extern "C" fn artichoke_securerandom_uuid(
    mrb: *mut sys::mrb_state,
    _slf: sys::mrb_value,
) -> sys::mrb_value {
    mrb_get_args!(mrb, none);
    let mut interp = unwrap_interpreter!(mrb);
    let mut guard = Guard::new(&mut interp);
    let result = trampoline::uuid(&mut guard);
    match result {
        Ok(value) => value.inner(),
        Err(exception) => exception::raise(guard, exception),
    }
}
