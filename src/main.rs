use clack_host::events::event_types::MidiEvent;
use clack_host::prelude::*;
use clack_host::process::StartedPluginAudioProcessor;
use jack::{AudioIn, AudioOut, ClientOptions, MidiIn, NotificationHandler, Port, ProcessHandler};
use miette::{IntoDiagnostic, Result};

mod host;

fn main() -> Result<()> {
    let (jack_client, _jack_client_status) =
        jack::Client::new("claphost", ClientOptions::empty()).into_diagnostic()?;
    let buffer_size = jack_client.buffer_size();
    let midi_in = jack_client
        .register_port("midi_in", MidiIn)
        .into_diagnostic()?;
    let input_fl = jack_client
        .register_port("input_FL", AudioIn)
        .into_diagnostic()?;
    let input_fr = jack_client
        .register_port("input_FR", AudioIn)
        .into_diagnostic()?;
    let output_fl = jack_client
        .register_port("output_FL", AudioOut)
        .into_diagnostic()?;
    let output_fr = jack_client
        .register_port("output_FR", AudioOut)
        .into_diagnostic()?;
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!(
            "Usage: {} path/to/plugin.clap [plugin-index]",
            std::env::args().nth(0).unwrap()
        );
        std::process::exit(1);
    };
    let plugin_index = std::env::args().nth(2).and_then(|index| index.parse().ok());
    let mut plugin = host::init(path, plugin_index)?;
    let stopped = plugin
        .activate(
            |_handle, _shared, _thread| host::AudioProcessor {},
            PluginAudioConfiguration {
                sample_rate: jack_client.sample_rate() as f64,
                frames_count_range: buffer_size..=buffer_size,
            },
        )
        .into_diagnostic()?;
    let started = stopped.start_processing().into_diagnostic()?;
    let _async_client = jack_client
        .activate_async(
            Notification {},
            Process {
                audio_processor: started,
                ports: Ports {
                    midi_in,
                    input_fl,
                    input_fr,
                    output_fl,
                    output_fr,
                },
                buffers: Buffers {
                    inputs: AudioPorts::with_capacity(2, 1),
                    outputs: AudioPorts::with_capacity(2, 1),
                    input_buffers: vec![vec![0.0; buffer_size as usize]; 2].into_boxed_slice(),
                    output_buffers: vec![vec![0.0; buffer_size as usize]; 2].into_boxed_slice(),
                    midi_in: Vec::with_capacity(256),
                },
            },
        )
        .into_diagnostic()?;
    loop {
        std::thread::park();
    }
}

struct Notification {}

impl NotificationHandler for Notification {
    fn thread_init(&self, _: &jack::Client) {}

    fn shutdown(&mut self, _status: jack::ClientStatus, _reason: &str) {}

    fn sample_rate(&mut self, _: &jack::Client, _srate: jack::Frames) -> jack::Control {
        jack::Control::Continue
    }

    fn xrun(&mut self, _: &jack::Client) -> jack::Control {
        jack::Control::Continue
    }
}

struct Process {
    ports: Ports,
    buffers: Buffers,
    audio_processor: StartedPluginAudioProcessor<host::ClapHost>,
}

impl ProcessHandler for Process {
    fn process(&mut self, _client: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let input_fl = self.ports.input_fl.as_slice(ps);
        let input_fr = self.ports.input_fr.as_slice(ps);
        let Buffers {
            inputs,
            outputs,
            input_buffers,
            output_buffers,
            midi_in,
        } = &mut self.buffers;
        input_buffers[0].copy_from_slice(input_fl);
        input_buffers[1].copy_from_slice(input_fr);
        for event in self.ports.midi_in.iter(ps).take(midi_in.capacity()) {
            let time = event.time;
            let header = EventHeader::new(time);
            if event.bytes.len() > 3 {
                dbg!(event);
                eprintln!("MIDI message was more than 3 bytes long, skipping.");
                continue;
            }
            let mut data = [0; 3];
            data[..event.bytes.len()].copy_from_slice(event.bytes);
            midi_in.push(MidiEvent::new(header, 0, data))
        }
        let input_plugin_buffers = inputs.with_input_buffers([AudioPortBuffer {
            channels: AudioPortBufferType::f32_input_only(
                input_buffers
                    .iter_mut()
                    .map(|buffer| InputChannel::variable(buffer)),
            ),
            latency: 0,
        }]);
        let mut output_plugin_buffers = outputs.with_output_buffers([AudioPortBuffer {
            channels: AudioPortBufferType::f32_output_only(
                output_buffers.iter_mut().map(|buffer| &mut **buffer),
            ),
            latency: 0,
        }]);
        let result = self.audio_processor.process(
            &input_plugin_buffers,
            &mut output_plugin_buffers,
            &InputEvents::from_buffer(&&midi_in[..]),
            &mut OutputEvents::void(),
            ps.last_frame_time() as i64,
            None,
            None,
        );
        midi_in.clear();
        self.ports
            .output_fl
            .as_mut_slice(ps)
            .copy_from_slice(&self.buffers.output_buffers[0]);
        self.ports
            .output_fr
            .as_mut_slice(ps)
            .copy_from_slice(&self.buffers.output_buffers[1]);
        match result {
            Ok(_status) => jack::Control::Continue,
            Err(error) => {
                eprintln!("{error:?}");
                std::process::exit(1);
            }
        }
    }

    fn buffer_size(&mut self, _: &jack::Client, size: jack::Frames) -> jack::Control {
        self.buffers.realloc(size as usize);
        jack::Control::Continue
    }
}

struct Ports {
    midi_in: Port<MidiIn>,
    input_fl: Port<AudioIn>,
    input_fr: Port<AudioIn>,
    output_fl: Port<AudioOut>,
    output_fr: Port<AudioOut>,
}

struct Buffers {
    inputs: AudioPorts,
    outputs: AudioPorts,
    input_buffers: Box<[Vec<f32>]>,
    output_buffers: Box<[Vec<f32>]>,
    midi_in: Vec<MidiEvent>,
}

impl Buffers {
    fn realloc(&mut self, new_size: usize) {
        for buffer in &mut *self.input_buffers {
            *buffer = vec![0.0; new_size];
        }
        for buffer in &mut *self.output_buffers {
            *buffer = vec![0.0; new_size];
        }
    }
}
