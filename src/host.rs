use std::{ffi::OsStr, marker::PhantomData};

use clack_host::prelude::*;
use miette::{IntoDiagnostic, Result};

pub fn init(
    path: impl AsRef<OsStr>,
    plugin_index: Option<u32>,
) -> Result<PluginInstance<ClapHost>> {
    let host_info = HostInfo::new(
        "claphost",
        "FreeFull",
        "http://github.com/FreeFull/claphost",
        "0.0.1",
    )
    .into_diagnostic()?;

    let bundle = clack_host::bundle::PluginBundle::load(path).unwrap();
    let factory = bundle
        .get_plugin_factory()
        .expect("Plugin bundle didn't contain plugin factory.");
    for descriptor in factory.plugin_descriptors() {
        println!(
            "{:?} {:?}",
            descriptor.id().unwrap(),
            descriptor.name().unwrap()
        );
    }
    let descriptor;
    if let Some(index) = plugin_index {
        if index < factory.plugin_count() {
            descriptor = factory.plugin_descriptor(index).unwrap();
        } else {
            panic!("Index out of bounds.")
        }
    } else {
        descriptor = factory.plugin_descriptor(0).unwrap();
    }
    let plugin = PluginInstance::<ClapHost>::new(
        |&()| Shared { data: PhantomData },
        |_| MainThread {},
        &bundle,
        descriptor.id().unwrap(),
        &host_info,
    )
    .into_diagnostic()?;
    Ok(plugin)
}

pub struct ClapHost {}
impl Host for ClapHost {
    type Shared<'a> = Shared<'a>;

    type MainThread<'a> = MainThread;

    type AudioProcessor<'a> = AudioProcessor;
}

pub struct Shared<'a> {
    pub data: PhantomData<&'a ()>,
}

impl<'a> HostShared<'a> for Shared<'a> {
    fn request_restart(&self) {
        // Ignore restart requests
    }

    fn request_process(&self) {
        todo!()
    }

    fn request_callback(&self) {
        //todo!()
    }
}

pub struct MainThread {}

impl<'a> HostMainThread<'a> for MainThread {}

pub struct AudioProcessor {}
impl<'a> HostAudioProcessor<'a> for AudioProcessor {}
