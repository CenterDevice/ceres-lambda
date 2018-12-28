use aws_watchtower::config::EncryptedFunctionConfig;

use clams::config::Config;
use std::path::PathBuf;
use structopt::StructOpt;

/// A basic example
#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
struct Opt {
    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: u8,

    /// Config file to validate
    #[structopt(name = "CONFIG_FILE", parse(from_os_str))]
    file: PathBuf,
}

fn main() {
    let args = Opt::from_args();

    let enc_config = EncryptedFunctionConfig::from_file(args.file).expect("Failed to read encrypted config file");
    if args.verbose > 1 {
        eprintln!("{:#?}", enc_config);
    }

    let config = enc_config.decrypt().expect("Failed to decrypt encrypted config file");
    if args.verbose > 0 {
        eprintln!("{:#?}", config);
    }

    println!("Config okay.");
}
