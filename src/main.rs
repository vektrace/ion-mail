mod account;
mod args;
mod folder;
mod mail;

use args::{AccountOperation, Cli, FolderOperation, MailOperation, Resource};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::process;

pub const APP_NAME: &str = "ion_mail";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub accounts: Vec<Account>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Account {
    pub id: u32, // just in case someone wants to have 4 bil accounts
    pub email: String,
    pub active: bool,
    pub smtp: Smtp,
    pub imap: Imap,
    pub oidc: Option<String>,
    pub scopes: Option<Vec<String>>,
    pub token_expiration: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Smtp {
    pub host: String,
    pub port: u16,
    pub security: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Imap {
    pub host: String,
    pub port: u16,
    pub security: String,
}

#[derive(Deserialize)]
pub struct MailConfig {
    smtp: Smtp,
    imap: Imap,
    oidc: String,
    scopes: Vec<String>,
}

#[derive(Deserialize)]
pub struct Endpoints {
    device_authorization_endpoint: String,
    authorization_endpoint: String,
    token_endpoint: String,
}

fn main() {
    let mut toml_p = dirs::config_local_dir()
        .unwrap_or_else(|| {
            eprintln!("Could not find config directory");
            process::exit(1);
        })
        .join("ion-mail");

    if !toml_p.exists() {
        fs::create_dir_all(&toml_p).unwrap_or_else(|err| {
            eprintln!("Failed to create directories: {}", err);
            process::exit(1);
        });
    }

    toml_p.push("config.toml");

    let toml_path: &str = toml_p.to_str().unwrap();

    let mut config = Config {
        accounts: Vec::new(),
    };

    if toml_p.exists() {
        let config_str = fs::read_to_string(toml_path).unwrap_or_else(|err| {
            eprintln!("Error when reading config: {}", err);
            process::exit(1);
        });

        config = toml::from_str(&config_str).unwrap_or_else(|err| {
            eprintln!("Error when parsing toml: {}", err);
            process::exit(1);
        });
    }

    #[cfg(target_os = "windows")]
    {
        let windows_store = windows_native_keyring_store::Store::new().unwrap_or_else(|err| {
            eprintln!("Unexpected Error: {}", err);
            process::exit(1);
        });
        keyring_core::set_default_store(windows_store);
    }
    #[cfg(target_os = "linux")]
    {
        let zbus_store = zbus_secret_service_keyring_store::Store::new().unwrap_or_else(|err| {
            eprintln!("Unexpected Error: {}", err);
            process::exit(1);
        });
        keyring_core::set_default_store(zbus_store);
    }

    let args = Cli::parse();

    match args.resource {
        Resource::Account { operation } => {
            match operation {
                AccountOperation::Add => account::add(toml_path, config),
                AccountOperation::List => account::list(config),
                AccountOperation::Use { account } => account::switch(toml_path, config, account),
                AccountOperation::Whoami => account::whoami(config),
                // reminder: since account is optional, in the edit function i have to do:
                // if let Some(account) = account {
                AccountOperation::Edit { account } => account::edit(toml_path, config, account),
                AccountOperation::Logout { account } => account::logout(toml_path, config, account),
            }
        }
        Resource::Mail { operation } => match operation {
            MailOperation::Send {
                to,
                subject,
                body,
                attachments,
                yes,
            } => mail::send(config, to, subject, body, attachments, yes),
            MailOperation::Read { folder, id } => mail::read(config, folder, id),
            MailOperation::Download {
                folder,
                id,
                attachment_id,
                save_folder,
            } => mail::download(config, folder, id, attachment_id, save_folder),
            MailOperation::Search { query, folder } => mail::search(config, query, folder),
            MailOperation::Move { from, to, id } => mail::mv(config, from, to, id),
            MailOperation::Delete { id, folder } => mail::delete(config, id, folder),
        },
        Resource::Folder { operation } => match operation {
            FolderOperation::List { stats } => folder::list(config, stats),
            FolderOperation::View { folder, page_size } => folder::view(config, folder, page_size),
            FolderOperation::Create { name, parents } => folder::create(config, name, parents),
            FolderOperation::Delete {
                name,
                recursive,
                yes,
            } => folder::delete(config, name, recursive, yes),
            FolderOperation::Empty { name } => folder::empty(config, name),
        },
    }
    keyring_core::unset_default_store();
}
