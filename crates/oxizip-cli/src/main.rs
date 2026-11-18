use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use oxizip_engine::{
    create, detect, extract, list, test_archive, CancellationToken, CoreError, CreateOptions,
    ExtractOptions, FormatId, ListOptions, MethodId, NullProgress, ZipCryptoKind,
};
use oxizip_i18n::{error_message, t, Lang};
use tracing_subscriber::EnvFilter;

mod exit {
    use std::process::ExitCode;
    pub fn ok() -> ExitCode {
        ExitCode::SUCCESS
    }
    pub fn failure() -> ExitCode {
        ExitCode::from(1)
    }
    pub fn canceled() -> ExitCode {
        ExitCode::from(2)
    }
}

#[derive(Parser, Debug)]
#[command(name = "oxizip", version, about = "Oxizip archive tool")]
struct Cli {
    /// Increase log verbosity
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(long, default_value = "zh-cn")]
    lang: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List archive entries
    #[command(visible_alias = "l")]
    List {
        archive: PathBuf,
        #[arg(long)]
        password: Option<String>,
    },
    /// Extract archive
    #[command(visible_alias = "x")]
    Extract {
        archive: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        password: Option<String>,
    },
    /// Create an archive (default format: zip)
    #[command(visible_alias = "c")]
    Create {
        archive: PathBuf,
        /// Format: zip, 7z, zipx, exe, tar, tgz, lzh, iso, gz, xz, zpaq
        #[arg(short = 'f', long, default_value = "zip")]
        format: String,
        /// Password (ZIP AES / 7z AES)
        #[arg(long, short = 'p')]
        password: Option<String>,
        /// Compression level 0-9 (where supported)
        #[arg(long, short = 'l')]
        level: Option<u32>,
        /// Method: store,deflate,bzip2,lzma,xz,zstd,lzma2,ppmd
        #[arg(long, short = 'm')]
        method: Option<String>,
        /// Split into volumes of this many bytes (e.g. 104857600)
        #[arg(long)]
        volume_size: Option<u64>,
        /// ZIP crypto: aes256 (default), aes128, aes192
        #[arg(long, default_value = "aes256")]
        zip_crypto: String,
        inputs: Vec<PathBuf>,
    },
    /// Test archive integrity
    #[command(visible_alias = "t")]
    Test {
        archive: PathBuf,
        #[arg(long)]
        password: Option<String>,
    },
    /// Detect archive format
    Detect { archive: PathBuf },
}

fn parse_lang(s: &str) -> Lang {
    match s.to_ascii_lowercase().as_str() {
        "en" | "en-us" | "english" => Lang::En,
        _ => Lang::ZhCn,
    }
}

fn map_exit(err: &CoreError) -> ExitCode {
    match err {
        CoreError::Canceled => exit::canceled(),
        _ => exit::failure(),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let lang = parse_lang(&cli.lang);

    let filter = match cli.verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_target(false)
        .init();

    let cancel = CancellationToken::new();
    let progress = NullProgress;

    let result = match cli.command {
        Commands::List { archive, password } => {
            let opts = ListOptions { password };
            list(&archive, &opts, &progress, &cancel).map(|entries| {
                for e in entries {
                    let kind = if e.is_dir { "DIR " } else { "FILE" };
                    let size = e
                        .uncompressed_size
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "-".into());
                    println!("{kind}\t{size}\t{}", e.name);
                }
            })
        }
        Commands::Extract {
            archive,
            output,
            password,
        } => {
            let mut opts = ExtractOptions::to_dir(output);
            opts.password = password;
            extract(&archive, &opts, &progress, &cancel)
        }
        Commands::Create {
            archive,
            format,
            password,
            level,
            method,
            volume_size,
            zip_crypto,
            inputs,
        } => {
            if inputs.is_empty() {
                eprintln!("create requires at least one input path");
                return exit::failure();
            }
            let Some(fmt) = FormatId::parse_create_name(&format) else {
                eprintln!(
                    "unknown format '{format}'. choose one of: {}",
                    oxizip_engine::create_format_names().join(", ")
                );
                return exit::failure();
            };
            let method = match method {
                Some(m) => match MethodId::parse(&m) {
                    Some(v) => Some(v),
                    None => {
                        eprintln!(
                            "unknown method '{m}'. try: store,deflate,bzip2,lzma,xz,zstd,lzma2,ppmd"
                        );
                        return exit::failure();
                    }
                },
                None => None,
            };
            let Some(zip_crypto) = ZipCryptoKind::parse(&zip_crypto) else {
                eprintln!("unknown zip-crypto '{zip_crypto}'. try: aes256,aes128,aes192");
                return exit::failure();
            };
            let opts = CreateOptions {
                password,
                level,
                method,
                volume_size,
                zip_crypto,
            };
            create(&archive, fmt, &inputs, &opts, &progress, &cancel)
        }
        Commands::Test { archive, password } => {
            let opts = ListOptions { password };
            test_archive(&archive, &opts, &progress, &cancel)
        }
        Commands::Detect { archive } => detect(&archive).map(|fmt| {
            println!("{}", fmt.as_str());
        }),
    };

    match result {
        Ok(()) => {
            println!("{}", t(lang, "cli.done"));
            exit::ok()
        }
        Err(err) => {
            eprintln!("{}", error_message(lang, &err));
            map_exit(&err)
        }
    }
}
