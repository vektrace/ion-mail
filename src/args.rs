/*
    ion-mail  Mail CLI in Rust supporting all mail functions & OAuth2 login
    Copyright (C) 2026  Vektrace

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU Affero General Public License as
    published by the Free Software Foundation, either version 3 of the
    License, or (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Affero General Public License for more details.

    You should have received a copy of the GNU Affero General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Mail CLI in Rust supporting all mail functions & OAuth2 login"
)]
pub struct Cli {
    #[command(subcommand)]
    pub resource: Resource,
}

#[derive(Subcommand)]
pub enum Resource {
    Account {
        #[command(subcommand)]
        operation: AccountOperation,
    },
    Mail {
        #[command(subcommand)]
        operation: MailOperation,
    },
    Folder {
        #[command(subcommand)]
        operation: FolderOperation,
    },
}

#[derive(Subcommand)]
pub enum AccountOperation {
    /// Add a new mail account
    Add,
    /// List all configured accounts
    ///
    /// Displays a list of all configured accounts.
    /// The currently active account (used by default for mail commands)
    /// will be highlighted.
    #[command(alias = "ls")]
    List,
    /// Switch the active account
    ///
    /// Sets the specified account as the default for all following
    /// mail and folder operations.
    Use {
        /// The ID or email of the account to activate
        account: String,
    },
    /// Display information about the currently active account
    ///
    /// Shows details for the account currently in use, including
    /// the email address, display name, and connection settings.
    Whoami,
    /// Modify an existing account's settings
    ///
    /// Update specific details like the name or server. If no flags are
    /// provided, the currently active account will be edited.
    Edit {
        /// The ID or email of the account to edit
        #[arg(short, long)]
        account: Option<String>,
    },
    /// Log out of an account and clear configuration
    ///
    /// This command removes the account configuration from your local machine.
    /// This action is IRREVERSIBLE and does not delete the actual
    /// mail on the server.
    Logout {
        /// The account ID or email to log out of (defaults to the currently active account)
        #[arg(short, long)]
        account: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum MailOperation {
    /// Construct and send a new email
    ///
    /// If required fields (to, subject, body) are missing from the flags,
    /// the application will prompt for them interactively.
    Send {
        /// Recipient email address(es)
        #[arg(short, long, value_delimiter = ',')]
        to: Option<Vec<String>>,
        /// The subject line of the email
        #[arg(short, long)]
        subject: Option<String>,
        /// The message content
        #[arg(short, long)]
        body: Option<String>,
        /// Path to files to attach (can be used multiple times)
        #[arg(short, long)]
        attachments: Option<Vec<std::path::PathBuf>>,
        /// Whether to skip final confirmation
        #[arg(short, long)]
        yes: bool,
    },
    /// View the full content of a specific email
    ///
    /// This command fetches the entire message (sender, subject, body) from
    /// the server using the index number provided by the 'folder view' command.
    Read {
        /// The folder where the email is located
        folder: String,
        /// The index number shown next to the email in 'folder view'
        id: u32,
    },
    /// Download the attachment(s) of a specific email
    ///
    /// This command downloads all attachments into the save folder.
    /// If the attachment ID is set, only the attachments with this index
    /// will be downloaded.
    Download {
        /// The folder where the email is located
        folder: String,
        /// The index number shown next to the email in 'folder view'
        id: u32,
        /// The index number shown next to the attachments when viewing an email
        #[arg(short, long, value_delimiter = ',')]
        attachment_id: Option<Vec<u32>>,
        /// The folder to save the attachments to
        save_folder: String,
    },
    /// Search for messages matching a specific sender or query
    ///
    /// This command performs a client-side search of your mail.
    /// It scans headers, subjects, and body content to find
    /// relevant matches.
    Search {
        /// The search terms or sender to look for
        #[arg(short, long)]
        query: String,
        /// The folder to search within
        ///
        /// Defaults to INBOX. Use 'ALL' to search across every
        /// folder available on the account.
        #[arg(short, long, default_value = "INBOX")]
        folder: String,
    },
    /// Move a message from one folder to another
    ///
    /// This command transfers a specific email between folders on the
    /// server.
    Move {
        /// The name of the folder where the message is currently located
        ///
        /// Examples: INBOX, Sent, Drafts.
        from: String,
        /// The name of the destination folder
        ///
        /// If the folder does not exist, the command will fail.
        to: String,
        /// The index number(s) of the message(s) to move
        #[arg(short, long, value_delimiter = ',')]
        id: Vec<u32>,
    },
    /// Delete a message
    ///
    /// This command permanently deletes a message. This action cannot
    /// be undone.
    Delete {
        /// The index number(s) of the message(s) to delete
        ///
        /// If not provided, multiple messages can be selected
        /// interactively.
        #[arg(short, long, value_delimiter = ',')]
        id: Option<Vec<u32>>,
        /// The folder where the message(s) are in
        #[arg(short, long)]
        folder: String,
    },
}

#[derive(Subcommand)]
pub enum FolderOperation {
    /// List all available mail folders on the server
    ///
    /// Fetches the complete directory tree of your mailbox. This includes
    /// standard folders (INBOX, Sent, Trash) and any custom labels or
    /// subfolders you have created.
    #[command(alias = "ls")]
    List {
        /// Show the number of messages and unread counts for each folder
        ///
        /// Note: Enabling this may take longer as the program must
        /// query the status of each folder individually.
        #[arg(short, long)]
        stats: bool,
    },
    /// View all messages within a specific folder
    ///
    /// This command retrieves the message infos (sender, subject, received date)
    /// from the requested folder and displays them in a list. It is the
    /// primary way to browse through your Mailboxes.
    View {
        /// The name of the folder to open
        ///
        /// Case-sensitivity depends on your mail provider.
        folder: String,
        /// Number of messages to show per page
        #[arg(short, long, default_value_t = 20)]
        page_size: usize,
    },
    /// Create a new folder on the mail server
    ///
    /// This command adds a new mailbox to your account. You can
    /// create top-level folders or subfolders by using the
    /// server's delimiter (usually a forward slash or a dot).
    Create {
        /// The name of the new folder to be created
        ///
        /// Example: 'Projects' or 'Work/Invoices'. Avoid using
        /// reserved names like 'INBOX'.
        name: String,
        /// Automatically create parent folders if they don't exist
        #[arg(short, long)]
        parents: bool,
    },
    /// Permanently remove a folder from the server
    ///
    /// This action will delete the folder and, depending on your server
    /// configuration, may also delete all messages contained within it.
    /// This cannot be undone.
    Delete {
        /// The name of the folder to delete
        name: Vec<String>,
        /// Delete the folder even if it contains subfolders
        #[arg(short, long)]
        recursive: bool,
        /// Skip the confirmation
        #[arg(short, long)]
        yes: bool,
    },
    /// Delete all messages within a specific folder
    ///
    /// This command permanently removes every email inside the target
    /// folder while keeping the folder structure intact. This is
    /// commonly used to clear out 'Trash' or 'Spam' folders.
    Empty {
        /// The name of the folder to empty
        name: String,
    },
}
