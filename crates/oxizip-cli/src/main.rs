use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use oxizip_engine::{
    add_to_archive, checksum_paths, create, delete_entries, detect, extract, list,
    recover_archive_password, rename_entry, repair_archive, test_archive, CancellationToken,
    CoreError, CreateOptions, ExtractOptions, FormatId, ListOptions, MethodId, NullProgress,
    PasswordRecoverOptions, ZipCryptoKind,
};
use oxizip_i18n::{error_message, t, Lang};
use tracing_subscriber::EnvFilter;

mod dest;
mod shell;

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
#[command(name = "oxizip-cli", version, about = "Oxizip archive tool")]
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
        /// Destination directory (required unless --here/--smart/--archive-name-dir)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Extract to the archive's parent directory
        #[arg(long, conflicts_with_all = ["smart", "archive_name_dir"])]
        here: bool,
        /// Smart extract (single root folder → here, else archive-name folder)
        #[arg(long, conflicts_with_all = ["here", "archive_name_dir"])]
        smart: bool,
        /// Extract into `<parent>/<archive-stem>/`
        #[arg(long, conflicts_with_all = ["here", "smart"])]
        archive_name_dir: bool,
        /// Only extract these entry names (repeatable)
        #[arg(long = "entry", value_name = "NAME")]
        entries: Vec<String>,
        /// Open the destination folder after success
        #[arg(long)]
        open_after: bool,
        /// Delete the archive after successful extract
        #[arg(long)]
        delete_archive: bool,
        #[arg(long)]
        password: Option<String>,
    },
    /// Create an archive (default format: zip)
    #[command(visible_alias = "c")]
    Create {
        /// Output archive path (optional with --beside)
        #[arg(required_unless_present = "beside")]
        archive: Option<PathBuf>,
        /// Place output beside the first input as `<name>.<ext>`
        #[arg(long)]
        beside: bool,
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
    /// Add files/folders into an existing ZIP/ZIPX/7z archive
    Add {
        archive: PathBuf,
        #[arg(long, short = 'p')]
        password: Option<String>,
        #[arg(long, short = 'm')]
        method: Option<String>,
        #[arg(long, short = 'l')]
        level: Option<u32>,
        #[arg(long, default_value = "aes256")]
        zip_crypto: String,
        inputs: Vec<PathBuf>,
    },
    /// Delete entries from an existing ZIP/ZIPX/7z archive
    Delete {
        archive: PathBuf,
        /// Entry names to delete (repeatable)
        #[arg(long = "entry", value_name = "NAME", required = true)]
        entries: Vec<String>,
        #[arg(long)]
        password: Option<String>,
    },
    /// Rename one entry inside an existing ZIP/ZIPX/7z archive
    Rename {
        archive: PathBuf,
        from: String,
        to: String,
        #[arg(long)]
        password: Option<String>,
    },
    /// Compute CRC32 and SHA-256 for one or more files
    Checksum {
        /// Files to hash
        paths: Vec<PathBuf>,
    },
    /// Repair a damaged ZIP/7z archive (best-effort)
    Repair {
        archive: PathBuf,
        /// Output path (default: `<stem>_repaired.<ext>` beside source)
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        password: Option<String>,
    },
    /// Recover a forgotten ZIP/7z password (wordlist + optional short brute)
    #[command(name = "recover-password")]
    RecoverPassword {
        archive: PathBuf,
        /// Newline-delimited password candidates
        #[arg(long)]
        wordlist: Option<PathBuf>,
        /// Charset for brute force (default: a-z0-9)
        #[arg(long, default_value = "abcdefghijklmnopqrstuvwxyz0123456789")]
        charset: String,
        /// Max brute length (0 = wordlist only; default 4)
        #[arg(long, default_value_t = 4)]
        max_len: u32,
    },
    /// Install / remove file-manager context actions (Windows / Linux)
    Shell {
        #[command(subcommand)]
        action: ShellCmd,
    },
}

#[derive(Subcommand, Debug)]
enum ShellCmd {
    /// Register context-menu / desktop actions for the current user
    Install,
    /// Remove Oxizip shell integration
    Uninstall,
    /// Show whether shell integration is present
    Status,
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

fn fail_msg(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    exit::failure()
}

fn parse_method(method: Option<String>) -> Result<Option<MethodId>, String> {
    match method {
        Some(m) => MethodId::parse(&m)
            .map(Some)
            .ok_or_else(|| {
                format!("unknown method '{m}'. try: store,deflate,bzip2,lzma,xz,zstd,lzma2,ppmd")
            }),
        None => Ok(None),
    }
}

fn main() -> ExitCode {
    enable_utf8_console();

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

    if let Commands::Shell { action } = &cli.command {
        let action = match action {
            ShellCmd::Install => shell::ShellAction::Install,
            ShellCmd::Uninstall => shell::ShellAction::Uninstall,
            ShellCmd::Status => shell::ShellAction::Status,
        };
        return shell::run(action, lang);
    }

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
            here,
            smart,
            archive_name_dir,
            entries,
            open_after,
            delete_archive,
            password,
        } => run_extract(
            archive,
            output,
            here,
            smart,
            archive_name_dir,
            entries,
            open_after,
            delete_archive,
            password,
            &progress,
            &cancel,
        ),
        Commands::Create {
            archive,
            beside,
            format,
            password,
            level,
            method,
            volume_size,
            zip_crypto,
            inputs,
        } => run_create(
            archive,
            beside,
            format,
            password,
            level,
            method,
            volume_size,
            zip_crypto,
            inputs,
            &progress,
            &cancel,
        ),
        Commands::Test { archive, password } => {
            let opts = ListOptions { password };
            test_archive(&archive, &opts, &progress, &cancel)
        }
        Commands::Detect { archive } => detect(&archive).map(|fmt| {
            println!("{}", fmt.as_str());
        }),
        Commands::Add {
            archive,
            password,
            method,
            level,
            zip_crypto,
            inputs,
        } => {
            if inputs.is_empty() {
                return fail_msg("add requires at least one input path");
            }
            let method = match parse_method(method) {
                Ok(m) => m,
                Err(e) => return fail_msg(&e),
            };
            let Some(zip_crypto) = ZipCryptoKind::parse(&zip_crypto) else {
                return fail_msg("unknown zip-crypto; try: aes256,aes128,aes192");
            };
            let opts = CreateOptions {
                password,
                level,
                method,
                volume_size: None,
                zip_crypto,
            };
            add_to_archive(&archive, &inputs, &opts, &progress, &cancel)
        }
        Commands::Delete {
            archive,
            entries,
            password,
        } => delete_entries(
            &archive,
            &entries,
            password.as_deref(),
            &progress,
            &cancel,
        ),
        Commands::Rename {
            archive,
            from,
            to,
            password,
        } => rename_entry(
            &archive,
            &from,
            &to,
            password.as_deref(),
            &progress,
            &cancel,
        ),
        Commands::Checksum { paths } => {
            if paths.is_empty() {
                return fail_msg("checksum requires at least one file path");
            }
            checksum_paths(&paths, &progress, &cancel).map(|results| {
                for cs in results {
                    println!(
                        "{}\tCRC32={}\tSHA256={}\t{} bytes",
                        cs.path,
                        cs.crc32_hex(),
                        cs.sha256_hex(),
                        cs.size
                    );
                }
            })
        }
        Commands::Repair {
            archive,
            output,
            password,
        } => repair_archive(
            &archive,
            output.as_deref(),
            password.as_deref(),
            &progress,
            &cancel,
        )
        .map(|out| {
            println!("{}", out.display());
        }),
        Commands::RecoverPassword {
            archive,
            wordlist,
            charset,
            max_len,
        } => {
            if wordlist.is_none() && max_len == 0 {
                return fail_msg("recover-password requires --wordlist and/or --max-len > 0");
            }
            let opts = PasswordRecoverOptions {
                wordlist,
                charset,
                max_len,
            };
            recover_archive_password(&archive, &opts, &progress, &cancel).map(|pw| {
                if pw.is_empty() {
                    println!("(empty password)");
                } else {
                    println!("{pw}");
                }
            })
        }
        Commands::Shell { .. } => unreachable!(),
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

fn run_extract(
    archive: PathBuf,
    output: Option<PathBuf>,
    here: bool,
    smart: bool,
    archive_name_dir: bool,
    entries: Vec<String>,
    open_after: bool,
    delete_archive: bool,
    password: Option<String>,
    progress: &NullProgress,
    cancel: &CancellationToken,
) -> Result<(), CoreError> {
    let layout = if here {
        dest::ExtractLayout::Here
    } else if smart {
        dest::ExtractLayout::Smart
    } else if archive_name_dir {
        dest::ExtractLayout::ArchiveName
    } else {
        dest::ExtractLayout::Explicit
    };

    let dest = match layout {
        dest::ExtractLayout::Explicit => output.ok_or_else(|| {
            CoreError::Other(
                "extract requires -o/--output, or one of --here / --smart / --archive-name-dir"
                    .into(),
            )
        })?,
        dest::ExtractLayout::Here => dest::dest_here(&archive),
        dest::ExtractLayout::ArchiveName => dest::dest_archive_name(&archive),
        dest::ExtractLayout::Smart => {
            let listed = list(
                &archive,
                &ListOptions {
                    password: password.clone(),
                },
                progress,
                cancel,
            )?;
            dest::dest_smart(&archive, &listed)
        }
    };

    fs::create_dir_all(&dest)?;
    let mut opts = ExtractOptions::to_dir(&dest);
    opts.password = password;
    if !entries.is_empty() {
        opts.entries = Some(entries);
    }
    extract(&archive, &opts, progress, cancel)?;

    if open_after {
        let _ = open::that(&dest);
    }
    if delete_archive {
        fs::remove_file(&archive)?;
    }
    Ok(())
}

fn run_create(
    archive: Option<PathBuf>,
    beside: bool,
    format: String,
    password: Option<String>,
    level: Option<u32>,
    method: Option<String>,
    volume_size: Option<u64>,
    zip_crypto: String,
    inputs: Vec<PathBuf>,
    progress: &NullProgress,
    cancel: &CancellationToken,
) -> Result<(), CoreError> {
    if inputs.is_empty() {
        return Err(CoreError::Other(
            "create requires at least one input path".into(),
        ));
    }
    let Some(fmt) = FormatId::parse_create_name(&format) else {
        return Err(CoreError::Other(format!(
            "unknown format '{format}'. choose one of: {}",
            oxizip_engine::create_format_names().join(", ")
        )));
    };
            let method = parse_method(method).map_err(CoreError::Other)?;
    let Some(zip_crypto) = ZipCryptoKind::parse(&zip_crypto) else {
        return Err(CoreError::Other(format!(
            "unknown zip-crypto '{zip_crypto}'. try: aes256,aes128,aes192"
        )));
    };

    let out = if let Some(path) = archive {
        path
    } else if beside {
        dest::default_create_path(&inputs[0], fmt.as_str())
    } else {
        return Err(CoreError::Other(
            "create requires an archive path, or --beside".into(),
        ));
    };

    let opts = CreateOptions {
        password,
        level,
        method,
        volume_size,
        zip_crypto,
    };
    create(&out, fmt, &inputs, &opts, progress, cancel)
}

/// Make Windows console print UTF-8 (zh-CN) instead of mojibake.
fn enable_utf8_console() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn SetConsoleOutputCP(code_page_id: u32) -> i32;
            fn SetConsoleCP(code_page_id: u32) -> i32;
        }
        const UTF8: u32 = 65001;
        unsafe {
            SetConsoleOutputCP(UTF8);
            SetConsoleCP(UTF8);
        }
    }
}
