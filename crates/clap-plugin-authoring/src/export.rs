use std::ffi::CStr;
use std::marker::PhantomData;

use clap_sys::{
    clap_host, clap_plugin, clap_plugin_factory_t, clap_process, clap_process_status,
    CLAP_PROCESS_ERROR,
};

use crate::author::{ActivationContext, AudioProcessor, Plugin, PluginDescriptor, PluginFactory};

#[allow(dead_code)]
#[doc(hidden)]
pub struct Instance<F: PluginFactory> {
    plugin: F::Plugin,
    descriptor: &'static PluginDescriptor,
}

impl<F: PluginFactory> Instance<F> {
    unsafe fn from_plugin<'a>(plugin: *const clap_plugin) -> &'a mut Self {
        let data = (*plugin).plugin_data as *mut Instance<F>;
        &mut *data
    }

    unsafe extern "C" fn init(plugin: *const clap_plugin) -> bool {
        let this = Self::from_plugin(plugin);
        this.plugin.init().is_ok()
    }

    unsafe extern "C" fn destroy(plugin: *const clap_plugin) {
        if plugin.is_null() {
            return;
        }
        let plugin = plugin as *mut clap_plugin;
        let data = (*plugin).plugin_data as *mut Instance<F>;
        if !data.is_null() {
            drop(Box::from_raw(data));
        }
        (*plugin).plugin_data = ::core::ptr::null_mut();
        drop(Box::from_raw(plugin));
    }

    unsafe extern "C" fn activate(
        plugin: *const clap_plugin,
        sample_rate: f64,
        min_frames_count: u32,
        max_frames_count: u32,
    ) -> bool {
        let this = Self::from_plugin(plugin);
        this.plugin
            .activate(&ActivationContext {
                sample_rate,
                min_frames_count,
                max_frames_count,
            })
            .is_ok()
    }

    unsafe extern "C" fn deactivate(plugin: *const clap_plugin) {
        let this = Self::from_plugin(plugin);
        this.plugin.deactivate();
    }

    unsafe extern "C" fn start_processing(plugin: *const clap_plugin) -> bool {
        let _ = Self::from_plugin(plugin);
        true
    }

    unsafe extern "C" fn stop_processing(plugin: *const clap_plugin) {
        let this = Self::from_plugin(plugin);
        this.plugin.reset();
    }

    unsafe extern "C" fn reset(plugin: *const clap_plugin) {
        let this = Self::from_plugin(plugin);
        this.plugin.reset();
    }

    unsafe extern "C" fn process(
        plugin: *const clap_plugin,
        process: *const clap_process,
    ) -> clap_process_status {
        let this = Self::from_plugin(plugin);
        if process.is_null() {
            return CLAP_PROCESS_ERROR.0 as clap_process_status;
        }
        let process = (process as *mut clap_process).as_mut().unwrap();
        this.plugin.process(process)
    }

    unsafe extern "C" fn get_extension(
        _plugin: *const clap_plugin,
        _id: *const i8,
    ) -> *const ::core::ffi::c_void {
        ::core::ptr::null()
    }

    unsafe extern "C" fn on_main_thread(plugin: *const clap_plugin) {
        let this = Self::from_plugin(plugin);
        this.plugin.on_main_thread();
    }
}

#[allow(dead_code)]
#[doc(hidden)]
pub struct FactoryShim<F: PluginFactory> {
    _marker: PhantomData<F>,
}

#[allow(dead_code)]
impl<F: PluginFactory> FactoryShim<F> {
    pub unsafe extern "C" fn get_plugin_count(_factory: *const clap_plugin_factory_t) -> u32 {
        F::descriptors().len() as u32
    }

    pub unsafe extern "C" fn get_plugin_descriptor(
        _factory: *const clap_plugin_factory_t,
        index: u32,
    ) -> *const clap_sys::clap_plugin_descriptor_t {
        F::descriptors()
            .get(index as usize)
            .map(|descriptor| descriptor.to_raw())
            .map(|raw| raw as *const _)
            .unwrap_or(::core::ptr::null())
    }

    pub unsafe extern "C" fn create_plugin(
        _factory: *const clap_plugin_factory_t,
        host: *const clap_host,
        plugin_id: *const i8,
    ) -> *const clap_plugin {
        if plugin_id.is_null() {
            return ::core::ptr::null();
        }
        let plugin_id = match CStr::from_ptr(plugin_id).to_str() {
            Ok(id) => id,
            Err(_) => return ::core::ptr::null(),
        };
        let descriptor = match F::descriptors().iter().find(|desc| desc.id == plugin_id) {
            Some(desc) => desc,
            None => return ::core::ptr::null(),
        };
        let plugin = match F::new_plugin(plugin_id, host) {
            Ok(plugin) => plugin,
            Err(err) => {
                log::error!("Failed to create CLAP plugin {}: {err}", plugin_id);
                return ::core::ptr::null();
            }
        };
        let instance = Box::new(Instance::<F> { plugin, descriptor });
        let raw = Box::new(clap_plugin {
            desc: descriptor.to_raw(),
            plugin_data: Box::into_raw(instance) as *mut _,
            init: Some(Instance::<F>::init),
            destroy: Some(Instance::<F>::destroy),
            activate: Some(Instance::<F>::activate),
            deactivate: Some(Instance::<F>::deactivate),
            start_processing: Some(Instance::<F>::start_processing),
            stop_processing: Some(Instance::<F>::stop_processing),
            reset: Some(Instance::<F>::reset),
            process: Some(Instance::<F>::process),
            get_extension: Some(Instance::<F>::get_extension),
            on_main_thread: Some(Instance::<F>::on_main_thread),
        });
        Box::into_raw(raw)
    }
}

#[macro_export]
macro_rules! clap_export {
    ($factory:path) => {
        static FACTORY: ::clap_sys::clap_plugin_factory_t = ::clap_sys::clap_plugin_factory_t {
            get_plugin_count: Some(<$crate::export::FactoryShim<$factory>>::get_plugin_count),
            get_plugin_descriptor: Some(
                <$crate::export::FactoryShim<$factory>>::get_plugin_descriptor,
            ),
            create_plugin: Some(<$crate::export::FactoryShim<$factory>>::create_plugin),
        };

        unsafe extern "C" fn __clap_entry_init(_path: *const ::core::ffi::c_char) -> bool {
            true
        }

        unsafe extern "C" fn __clap_entry_deinit() {}

        unsafe extern "C" fn __clap_entry_get_factory(
            factory_id: *const ::core::ffi::c_char,
        ) -> *const ::core::ffi::c_void {
            if factory_id.is_null() {
                return ::core::ptr::null();
            }
            let id = ::std::ffi::CStr::from_ptr(factory_id);
            if id.to_bytes() == b"clap.plugin-factory" {
                &FACTORY as *const _ as *const ::core::ffi::c_void
            } else {
                ::core::ptr::null()
            }
        }

        #[no_mangle]
        pub static clap_entry: ::clap_sys::clap_plugin_entry_t = ::clap_sys::clap_plugin_entry_t {
            clap_version: ::clap_sys::CLAP_VERSION_LATEST,
            init: Some(__clap_entry_init),
            deinit: Some(__clap_entry_deinit),
            get_factory: Some(__clap_entry_get_factory),
        };
    };
}
