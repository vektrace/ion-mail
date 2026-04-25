mod args;
mod account;
mod mail;
mod folder;

use args::{Cli, Resource, AccountOperation, MailOperation, FolderOperation};
use serde::{Serialize, Deserialize};
use clap::Parser;
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
    pub smtp: String,
    pub smtp_port: u16,
    pub imap: String,
    pub imap_port: u16,
}

fn main() {
    // somewhere load all accounts out of keyring
    // needs new param for like every function...
    // but lets worry about that later :)
    //
    // uh oh... i think it is time to worry now

    let toml_p = dirs::home_dir()
        .ok_or_else(|| {eprintln!("Could not find home directory"); process::exit(1)});

    let mut toml_path_unwrap = toml_p.unwrap();

    toml_path_unwrap.push(".ion-mail");

    if !toml_path_unwrap.exists() {
        fs::create_dir_all(&toml_path_unwrap).expect("Could not create directories");
    }

    toml_path_unwrap.push("config.toml");

    let toml_path: &str = toml_path_unwrap.to_str().expect("Path invalid");

    let mut config = Config {
        accounts: Vec::new(),
    };

    if toml_path_unwrap.exists() {
        let config_str = fs::read_to_string(toml_path).unwrap();

        config = toml::from_str(&config_str).expect("Invalid TOML file");
    }

    let args = Cli::parse();

    match args.resource {
        Resource::Account { operation } => {
            match operation {
                AccountOperation::Add => account::add(toml_path, config),
                AccountOperation::List => account::list(config),
                AccountOperation::Use {account} => account::switch(toml_path, config, account),
                AccountOperation::Whoami => account::whoami(config),
                // reminder: since account is optional, in the edit function i have to do:
                // if let Some(account) = account {
                AccountOperation::Edit { account } => account::edit(toml_path, config, account),
                AccountOperation::Logout { account } => account::logout(toml_path, config, account),
            }
            },
            Resource::Mail { operation } => {
                match operation {
                    MailOperation::Send { to, subject, body, attachments, yes } => mail::send(config, to, subject, body, attachments, yes),
                    MailOperation::Read { folder, id } => mail::read(config, folder, id),
                    MailOperation::Search { query, folder, since } => mail::search(config, query, folder, since),
                    MailOperation::Move { id, from, to } => mail::mv(config, id, from, to),
                    MailOperation::Draft { to, subject, body, attachments } => mail::draft(config, to, subject, body, attachments),
                }
            },
            Resource::Folder { operation } => {
                match operation {
                    FolderOperation::List { stats } => folder::list(config, stats),
                    FolderOperation::View { folder, page_size } => folder::view(config, folder, page_size),
                    FolderOperation::Create { name, parents } => folder::create(config, name, parents),
                    FolderOperation::Delete { name, recursive, yes } => folder::delete(config, name, recursive, yes),
                    FolderOperation::Empty { name } => folder::empty(config, name),
                }
            },
        }
    }
