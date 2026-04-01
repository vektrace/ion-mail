mod args;
mod account;
mod mail;
mod folder;

use args::{Cli, Resource, AccountOperation, MailOperation, FolderOperation};
use clap::Parser;

fn main() {
    let args = Cli::parse();

    match args.resource {
        Resource::Account { operation } => {
            match operation {
                AccountOperation::Add => account::add(),
                AccountOperation::List => account::list(),
                AccountOperation::Use {account} => account::switch(account),
                AccountOperation::Whoami => account::whoami(),
                // reminder: since account is optional, in the edit function i have to do:
                // if let Some(account) = account {
                AccountOperation::Edit { account } => account::edit(account),
                AccountOperation::Logout { account } => account::logout(account),
            }
        },
        Resource::Mail { operation } => {
            match operation {
                MailOperation::Send { to, subject, body, attachments, yes } => mail::send(to, subject, body, attachments, yes),
                MailOperation::List { limit, unread, json } => mail::list(limit, unread, json),
                MailOperation::Read { id } => mail::read(id),
                MailOperation::Search { query, folder, since } => mail::search(query, folder, since),
                MailOperation::Move { id, from, to } => mail::mv(id, from, to),
                MailOperation::Draft { to, subject, body, attachments } => mail::draft(to, subject, body, attachments),
            }
        },
        Resource::Folder { operation } => {
            match operation {
                FolderOperation::List { stats } => folder::list(stats),
                FolderOperation::View { folder, page_size } => folder::view(folder, page_size),
                FolderOperation::Create { name, parents } => folder::create(name, parents),
                FolderOperation::Delete { name, recursive } => folder::delete(name, recursive),
                FolderOperation::Empty { name } => folder::empty(name),
            }
        },
    }
}
