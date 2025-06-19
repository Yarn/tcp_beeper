
use std::iter::Iterator;
use std::collections::VecDeque;
use std::sync::{ Arc, atomic::{ AtomicU32, Ordering::Relaxed } };
use std::io::prelude::*;
use std::net::{ TcpStream, ToSocketAddrs as _ };

use miette::IntoDiagnostic;
use clap::Parser;

use cpal::traits::{ DeviceTrait, HostTrait, StreamTrait };
use cpal::{ Device, Sample };
use ringbuf::HeapRb;
use ringbuf::traits::{ Split, Consumer, Producer };

fn start_loop(addr: &str, output_device: Device, volume_mult: f32, rate_threshold: Option<f32>) -> miette::Result<()> {
    let config = output_device.default_output_config().into_diagnostic()?;
    let sample_rate = config.sample_rate().0;
    
    let ring = HeapRb::<f32>::new(8192);
    let (mut producer, mut consumer) = ring.split();
    
    let write_data = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
        let mut last = Sample::EQUILIBRIUM;
        for sample in data {
            *sample = consumer.try_pop().unwrap_or(last);
            last = *sample;
        }
    };
    
    let err_fn = |err| eprintln!("an error occurred on the output audio stream: {}", err);
    
    let output_stream = output_device.build_output_stream(&config.config(), write_data, err_fn, None).into_diagnostic()?;
    
    output_stream.play().into_diagnostic()?;
    
    let freq = 220;
    
    let pending_beep = Arc::new(AtomicU32::new(0));
    let mut _pending_beep = Arc::clone(&pending_beep);
    let addr = addr.to_string();
    let net_thread_handle = std::thread::spawn(move || {
        let timeout = ::std::time::Duration::from_millis(800);
        let addr = addr.to_socket_addrs().into_diagnostic()?.next().ok_or_else(|| miette::miette!("Invalid address"))?;
        let stream = TcpStream::connect_timeout(&addr, timeout).into_diagnostic()?;
        stream.set_read_timeout(Some(timeout)).into_diagnostic()?;
        for _ in stream.bytes() {
            _pending_beep.fetch_add(1, Relaxed);
        }
        
        let res: miette::Result<()> = Ok(());
        res
    });
    
    const SLEEP_TIME: ::std::time::Duration = ::std::time::Duration::from_millis(8);
    let beep_length = 0.1;
    let mut beep_remaining = 0.;
    
    let period_samples: u32 = sample_rate / freq;
    
    let mut send = |val| {
        loop {
            match producer.try_push(val) {
                Ok(_) => break,
                Err(_) => ::std::thread::sleep(SLEEP_TIME),
            }
        }
    };
    
    let mut rate_buf: VecDeque<u32> = VecDeque::with_capacity(40);
    rate_buf.resize(40, 0);
    let bin_period = 0.1;
    let mut bin_time = 0.;
    
    loop {
        if net_thread_handle.is_finished() {
            return net_thread_handle.join().expect("thread join");
        }
        
        if bin_time > bin_period {
            bin_time -= bin_period;
            rate_buf.pop_front();
            rate_buf.push_back(0);
        }
        
        let count = pending_beep.swap(0, Relaxed);
        *rate_buf.back_mut().unwrap() += count;
        
        let rate = rate_buf.iter().sum::<u32>() as f32 / (rate_buf.len() as f32 * bin_period);
        if count > 0 && rate_threshold.map(|x| rate >= x).unwrap_or(true) {
            beep_remaining = beep_length;
        }
        
        if beep_remaining > 0. {
            let mut val;
            for t in 0..period_samples {
                val = t as f32 /(sample_rate as f32) * (freq as f32) * 2. * std::f32::consts::PI;
                val = val.sin();
                val = val * 0.005 * volume_mult;
                send(val);
            }
            let dt = period_samples as f32 / sample_rate as f32;
            beep_remaining -= dt;
            bin_time += dt;
        } else {
            send(0.);
            bin_time += 1. / sample_rate as f32;
        }
    }
}

/// Beeps when a byte is received from a tcp server
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    
    /// Volume multiplier
    #[arg(short, long, default_value_t = 1.)]
    volume: f32,
    
    /// Minimum rate that must be reached before beeping starts (hz)
    /// uses a 4 second window
    #[arg(short, long)]
    min_rate: Option<f32>,
    
    /// tcp host in form <addr>:<port>
    #[arg()]
    addr: String,
}

fn main() -> miette::Result<()> {
    let args = Args::parse();
    
    let host = cpal::default_host();
    
    let output_device = host.default_output_device().expect("failed to get default output device");
    eprintln!("Using output device: \"{}\"", output_device.name().into_diagnostic()?);
    
    start_loop(&args.addr, output_device, args.volume, args.min_rate)?;
    
    Ok(())
}
