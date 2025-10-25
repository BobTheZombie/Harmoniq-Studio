use std::ffi::CStr;

use clap_sys::clap_param_info_t;

/// Lightweight wrapper for a CLAP parameter value.
#[derive(Clone, Copy, Debug)]
pub struct ParamValue {
    pub id: u32,
    pub value: f64,
}

/// Provides query utilities for CLAP parameters.
pub struct ParameterQuery<'a> {
    raw: &'a clap_param_info_t,
}

impl<'a> ParameterQuery<'a> {
    pub fn new(raw: &'a clap_param_info_t) -> Self {
        Self { raw }
    }

    pub fn name(&self) -> &str {
        unsafe { CStr::from_ptr(self.raw.name.as_ptr()) }
            .to_str()
            .unwrap_or("")
    }

    pub fn id(&self) -> u32 {
        self.raw.id
    }
}
