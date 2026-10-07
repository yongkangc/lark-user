use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "lark-user",
    version,
    about = "Enterprise Lark user-session tools. Live international protocol is not yet verified."
)]
pub struct Cli {
    #[arg(long, global = true, default_value = "enterprise")]
    pub profile: String,
    #[arg(long, global = true, env = "LARK_USER_STORE_DIR")]
    pub store_dir: Option<PathBuf>,
    #[arg(long, global = true, env = "LARK_USER_KEY_FILE")]
    pub key_file: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        help = "Emit compact JSON; output is JSON by default"
    )]
    pub json: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Auth {
        #[command(subcommand)]
        command: Auth,
    },
    Profile {
        #[command(subcommand)]
        command: Profiles,
    },
    Chats {
        #[command(subcommand)]
        command: Chats,
    },
    Messages {
        #[command(subcommand)]
        command: Messages,
    },
    Threads {
        #[command(subcommand)]
        command: Threads,
    },
    Archive {
        #[command(subcommand)]
        command: Archives,
    },
    Protocol {
        #[command(subcommand)]
        command: Protocol,
    },
    Capabilities,
    Sync {
        #[arg(long)]
        chat: String,
        #[arg(long)]
        since: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    Export {
        #[arg(long)]
        chat: String,
        #[arg(long,value_enum,default_value_t=ExportFormat::Jsonl)]
        format: ExportFormat,
        #[arg(long)]
        out: PathBuf,
        #[command(flatten)]
        query: Query,
    },
    Watch {
        #[arg(long)]
        chat: String,
        #[arg(long)]
        jsonl: bool,
    },
    Context {
        #[arg(long)]
        chat: Option<String>,
        #[arg(long, default_value_t = 65536)]
        max_bytes: usize,
        #[command(flatten)]
        query: Query,
    },
}

#[derive(Subcommand)]
pub enum Auth {
    Import {
        #[arg(
            long,
            help = "Private Playwright cookie JSON, or - for stdin; raw values are never arguments"
        )]
        cookie_file: PathBuf,
        #[command(flatten)]
        config: Configure,
    },
    Status,
    Logout,
    Keygen {
        #[arg(
            long,
            help = "Create a new 32-byte 0600 key in a private directory; never overwrite"
        )]
        out: PathBuf,
    },
}

#[derive(Args, Default)]
pub struct Configure {
    #[arg(long)]
    pub web_url: Option<String>,
    #[arg(long, help = "User-declared account ID; not live-verified")]
    pub account_id: Option<String>,
    #[arg(long, help = "User-declared tenant ID; not live-verified")]
    pub tenant_id: Option<String>,
    #[arg(long)]
    pub retention_days: Option<u32>,
}

#[derive(Subcommand)]
pub enum Profiles {
    Configure(Configure),
    Show,
}

#[derive(Args)]
pub struct Query {
    #[arg(long, help = "Explicitly use only the local archive")]
    pub local: bool,
    #[arg(long, help = "RFC3339 timestamp or Unix seconds")]
    pub since: Option<String>,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    #[arg(long)]
    pub cursor: Option<String>,
}

#[derive(Subcommand)]
pub enum Chats {
    List {
        #[arg(long)]
        local: bool,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long)]
        cursor: Option<String>,
    },
    Get {
        chat_id: String,
        #[arg(long)]
        local: bool,
    },
}

#[derive(Subcommand)]
pub enum Messages {
    List {
        #[arg(long)]
        chat: String,
        #[command(flatten)]
        query: Query,
    },
    Get {
        message_id: String,
        #[arg(long)]
        local: bool,
    },
    Search {
        query_text: String,
        #[arg(long)]
        chat: Option<String>,
        #[command(flatten)]
        query: Query,
    },
    Send {
        #[arg(long)]
        chat: String,
        #[arg(long)]
        text_file: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    Reply {
        #[arg(long)]
        message: String,
        #[arg(long)]
        text_file: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum Threads {
    Get {
        thread_id: String,
        #[command(flatten)]
        query: Query,
    },
}

#[derive(Subcommand)]
pub enum Archives {
    Import {
        #[arg(
            long,
            help = "Private normalized JSONL, or - for stdin; not a Lark protocol response"
        )]
        file: PathBuf,
    },
    Coverage {
        #[arg(long)]
        chat: Option<String>,
    },
    Prune {
        #[arg(
            long,
            help = "RFC3339 or Unix seconds; defaults to profile retention cutoff"
        )]
        before: Option<String>,
    },
    Clear {
        #[arg(
            long,
            help = "Explicitly approve removal of all messages from this local profile archive"
        )]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum Protocol {
    Inspect {
        #[arg(
            long,
            help = "Private local HAR; request bodies/header values are never printed or replayed"
        )]
        har: PathBuf,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ExportFormat {
    Jsonl,
}
