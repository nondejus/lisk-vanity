#![feature(int_to_from_bytes)]
#![feature(try_from)]

use std::f64;
use std::process;
use std::sync::atomic;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

extern crate clap;
extern crate digest;
extern crate ed25519_dalek;
extern crate hex;
extern crate num_bigint;
extern crate num_cpus;
extern crate sha2;

extern crate rand;
use rand::{OsRng, Rng};

extern crate num_traits;
use num_traits::ToPrimitive;

#[cfg(feature = "gpu")]
extern crate ocl;

mod cpu;
use cpu::bip39::entropy_to_mnemonic;

mod derivation;
use derivation::{cut_last_16, pubkey_to_address, secret_to_pubkey, GenerateKeyType};

mod pubkey_matcher;
use pubkey_matcher::{max_address, PubkeyMatcher};

#[cfg(feature = "gpu")]
mod gpu;
#[cfg(feature = "gpu")]
use gpu::Gpu;

#[cfg(not(feature = "gpu"))]
struct Gpu;

#[cfg(not(feature = "gpu"))]
impl Gpu {
    pub fn new(_: usize, _: usize, _: usize, _: &Matcher, _: bool) -> Result<Gpu, String> {
        eprintln!("GPU support has been disabled at compile time.");
        eprintln!("Rebuild with \"--features gpu\" to enable GPU support.");
        process::exit(1);
    }

    pub fn compute(&mut self, _: &mut [u8], _: &[u8]) -> Result<bool, String> {
        unreachable!()
    }
}

fn print_solution(
    secret_key_material: [u8; 32],
    secret_key_type: GenerateKeyType,
    public_key: [u8; 32],
    simple_output: bool,
) {
    if simple_output {
        println!(
            "{} {}",
            hex::encode_upper(&secret_key_material as &[u8]),
            pubkey_to_address(&public_key),
        );
    } else {
        match secret_key_type {
            GenerateKeyType::LiskPassphrase => println!(
                "Found matching account!\nPrivate Key: {}\nAddress:     {}",
                String::from_utf8(entropy_to_mnemonic(cut_last_16(&secret_key_material))).unwrap(),
                full_address(pubkey_to_address(&public_key)),
            ),
            GenerateKeyType::PrivateKey => println!(
                "Found matching account!\nPrivate Key: {}{}\nAddress:     {}",
                hex::encode_upper(&secret_key_material as &[u8]),
                hex::encode_upper(&public_key),
                full_address(pubkey_to_address(&public_key)),
            ),
        }
    }
}

struct ThreadParams {
    limit: usize,
    found_n: Arc<AtomicUsize>,
    output_progress: bool,
    attempts: Arc<AtomicUsize>,
    simple_output: bool,
    generate_key_type: GenerateKeyType,
    matcher: Arc<PubkeyMatcher>,
}

fn full_address(address: u64) -> String {
    return format!("{}L", address);
}

fn check_solution(params: &ThreadParams, key_material: [u8; 32]) -> bool {
    let public_key = secret_to_pubkey(key_material, params.generate_key_type);
    let matches = params.matcher.matches(&public_key);
    if matches {
        if params.output_progress {
            eprintln!("");
        }
        print_solution(
            key_material,
            params.generate_key_type,
            public_key,
            params.simple_output,
        );
        if params.limit != 0
            && params.found_n.fetch_add(1, atomic::Ordering::Relaxed) + 1 >= params.limit
        {
            process::exit(0);
        }
    }
    matches
}

fn main() {
    let args = clap::App::new("lisk-vanity")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Lee Bousfield <ljbousfield@gmail.com>")
        .about("Generate short Lisk addresses")
        .arg(
            clap::Arg::with_name("length")
                .value_name("LENGTH")
                .default_value("14")
                .required_unless("suffix")
                .help("The max length for the address"),
        )
        .arg(
            clap::Arg::with_name("generate_keypair")
                .short("k")
                .long("generate-keypair")
                .help("Generate a key pair instead of a passphrase"),
        )
        .arg(
            clap::Arg::with_name("threads")
                .short("t")
                .long("threads")
                .value_name("N")
                .help("The number of threads to use [default: number of cores minus one]"),
        )
        .arg(
            clap::Arg::with_name("gpu")
                .short("g")
                .long("gpu")
                .help("Enable use of the GPU through OpenCL"),
        )
        .arg(
            clap::Arg::with_name("limit")
                .short("l")
                .long("limit")
                .value_name("N")
                .default_value("1")
                .help("Generate N addresses, then exit (0 for infinite)"),
        )
        .arg(
            clap::Arg::with_name("gpu_threads")
                .long("gpu-threads")
                .value_name("N")
                .default_value("1048576")
                .help("The number of GPU threads to use"),
        )
        .arg(
            clap::Arg::with_name("no_progress")
                .long("no-progress")
                .help("Disable progress output"),
        )
        .arg(
            clap::Arg::with_name("simple_output")
                .long("simple-output")
                .help("Output found keys in the form \"[key] [address]\""),
        )
        .arg(
            clap::Arg::with_name("gpu_platform")
                .long("gpu-platform")
                .value_name("INDEX")
                .default_value("0")
                .help("The GPU platform to use"),
        )
        .arg(
            clap::Arg::with_name("gpu_device")
                .long("gpu-device")
                .value_name("INDEX")
                .default_value("0")
                .help("The GPU device to use"),
        )
        .get_matches();

    let max_length = args
        .value_of("length")
        .unwrap()
        .parse()
        .expect("Failed to parse LENGTH");

    let matcher_base = PubkeyMatcher::new(max_length);
    let estimated_attempts = matcher_base.estimated_attempts();
    let matcher_base = Arc::new(matcher_base);
    let limit = args
        .value_of("limit")
        .unwrap()
        .parse()
        .expect("Failed to parse limit option");
    let found_n_base = Arc::new(AtomicUsize::new(0));
    let attempts_base = Arc::new(AtomicUsize::new(0));
    let output_progress = !args.is_present("no_progress");
    let simple_output = args.is_present("simple_output");
    let _generate_passphrase = args.is_present("generate_passphrase");

    let gen_key_type;
    if args.is_present("generate_keypair") {
        gen_key_type = GenerateKeyType::PrivateKey;
    } else {
        gen_key_type = GenerateKeyType::LiskPassphrase;
    }

    let threads = args
        .value_of("threads")
        .map(|s| s.parse().expect("Failed to parse thread count option"))
        .unwrap_or_else(|| num_cpus::get() - 1);
    let mut thread_handles = Vec::with_capacity(threads);
    eprintln!("Estimated attempts needed: {}", estimated_attempts);
    for _ in 0..threads {
        let mut rng = OsRng::new().expect("Failed to get RNG for seed");
        let mut key_or_seed = [0u8; 32];
        rng.fill_bytes(&mut key_or_seed);
        let params = ThreadParams {
            limit,
            output_progress,
            simple_output,
            generate_key_type: gen_key_type.clone(),
            matcher: matcher_base.clone(),
            found_n: found_n_base.clone(),
            attempts: attempts_base.clone(),
        };
        thread_handles.push(thread::spawn(move || loop {
            if check_solution(&params, key_or_seed) {
                rng.fill_bytes(&mut key_or_seed);
            } else {
                if output_progress {
                    params.attempts.fetch_add(1, atomic::Ordering::Relaxed);
                }
                for byte in key_or_seed.iter_mut().rev() {
                    *byte = byte.wrapping_add(1);
                    if *byte != 0 {
                        break;
                    }
                }
            }
        }));
    }
    let mut gpu_thread = None;
    if args.is_present("gpu") {
        let gpu_platform = args
            .value_of("gpu_platform")
            .unwrap()
            .parse()
            .expect("Failed to parse GPU platform index");
        let gpu_device = args
            .value_of("gpu_device")
            .unwrap()
            .parse()
            .expect("Failed to parse GPU device index");
        let gpu_threads = args
            .value_of("gpu_threads")
            .unwrap()
            .parse()
            .expect("Failed to parse GPU threads option");
        let mut key_base = [0u8; 32];
        let params = ThreadParams {
            limit,
            output_progress,
            simple_output,
            generate_key_type: gen_key_type.clone(),
            matcher: matcher_base.clone(),
            found_n: found_n_base.clone(),
            attempts: attempts_base.clone(),
        };
        let mut gpu = Gpu::new(
            gpu_platform,
            gpu_device,
            gpu_threads,
            max_address(max_length),
            gen_key_type,
        )
        .unwrap();
        gpu_thread = Some(thread::spawn(move || {
            let mut rng = OsRng::new().expect("Failed to get RNG for seed");
            loop {
                rng.fill_bytes(&mut key_base);
                let found = gpu
                    .compute(&key_base as _)
                    .expect("Failed to run GPU computation");
                if output_progress {
                    params
                        .attempts
                        .fetch_add(gpu_threads, atomic::Ordering::Relaxed);
                }

                if let Some(found_private_key) = found {
                    if !check_solution(&params, found_private_key) {
                        eprintln!(
                            "GPU returned non-matching solution: {}",
                            hex::encode_upper(&found_private_key)
                        );
                    }
                } else {
                    // just continue
                }
            }
        }));
    }
    if output_progress {
        let start_time = Instant::now();
        let attempts = attempts_base;
        thread::spawn(move || loop {
            let attempts = attempts.load(atomic::Ordering::Relaxed);
            let estimated_percent =
                100. * (attempts as f64) / estimated_attempts.to_f64().unwrap_or(f64::INFINITY);
            let runtime = start_time.elapsed();
            let keys_per_second = (attempts as f64)
                // simplify to .as_millis() when available
                / (runtime.as_secs() as f64 + runtime.subsec_millis() as f64 / 1000.0);
            eprint!(
                "\rTried {} keys (~{:.2}%; {:.1} keys/s)",
                attempts, estimated_percent, keys_per_second,
            );
            thread::sleep(Duration::from_millis(100));
        });
    }
    if let Some(gpu_thread) = gpu_thread {
        gpu_thread.join().expect("Failed to join GPU thread");
    }
    for handle in thread_handles {
        handle.join().expect("Failed to join thread");
    }
    eprintln!("No computation devices specified");
    process::exit(1);
}
