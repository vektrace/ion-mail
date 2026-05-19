use crate::{APP_NAME, Account, Config};

use dialoguer::{Confirm, Input, Password, Select, theme::ColorfulTheme};
use email_address::EmailAddress;
use keyring_core::Entry;
use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::process;
use std::time::Duration;

pub fn auth(config: Config) -> imap::Session<native_tls::TlsStream<std::net::TcpStream>> {
    let mut found = false;

    let mut use_account = Account {
        id: 0,
        active: false,
        email: "".to_string(),
        smtp: "".to_string(),
        smtp_port: 0,
        imap: "".to_string(),
        imap_port: 0,
    };

    for account in config.accounts {
        if account.active {
            found = true;
            use_account = account;
        }
    }

    if !found {
        println!("No account is currently active");
        keyring_core::unset_default_store();
        process::exit(1);
    }

    let entry = match Entry::new(APP_NAME, &use_account.id.to_string()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Unexpected Error: {}", e);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    };
    let password = match entry.get_password() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to get password: {}", e);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    };

    let tls = native_tls::TlsConnector::builder().build().unwrap();

    let client = if let Ok(client) = imap::connect(
        (use_account.imap.clone(), use_account.imap_port),
        &use_account.imap,
        &tls,
    ) {
        client
    } else if let Ok(client) = imap::connect_starttls(
        (use_account.imap.clone(), use_account.imap_port),
        &use_account.imap,
        &tls,
    ) {
        client
    } else {
        eprintln!("Failed to connect via TLS and STARTTLS");
        keyring_core::unset_default_store();
        process::exit(1);
    };

    match client.login(use_account.email, password) {
        Ok(s) => {
            return s;
        }
        Err((e, _orig_client)) => {
            eprintln!("Login failed: {}", e);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    }
}

pub fn add(toml_path: &str, old_config: Config) {
    if old_config.accounts.len() >= (u32::MAX - 10).try_into().unwrap() {
        eprintln!("Too many accounts registered, remove some then try again");
        keyring_core::unset_default_store();
        process::exit(1);
    }
    let smtp: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("SMTP Server")
        .validate_with({
            move |input: &String| -> Result<(), &str> {
                let target = format!("{}:0", input);

                match target.to_socket_addrs() {
                    Ok(_) => return Ok(()),
                    Err(_) => Err("Hostname could not be resolved"),
                }
            }
        })
        .interact_text()
        .unwrap();

    let smtp_value = smtp.clone();
    let smtp_port: u16 = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("SMTP Port")
        .with_initial_text("587")
        .validate_with({
            move |input: &u16| -> Result<(), String> {
                let target = format!("{}:{}", smtp_value, input);

                if let Ok(mut addrs) = target.to_socket_addrs() {
                    if let Some(address) = addrs.next() {
                        if TcpStream::connect_timeout(&address, Duration::from_secs(3)).is_ok() {
                            return Ok(());
                        }
                    }
                }
                Err(format!("Could not connect to port {}", input))
            }
        })
        .interact_text()
        .unwrap();

    let imap: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("IMAP Server")
        .validate_with({
            move |input: &String| -> Result<(), &str> {
                let target = format!("{}:0", input);

                match target.to_socket_addrs() {
                    Ok(_) => return Ok(()),
                    Err(_) => Err("Hostname could not be resolved"),
                }
            }
        })
        .interact_text()
        .unwrap();

    let imap_value = imap.clone();
    let imap_port: u16 = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("IMAP Port")
        .with_initial_text("993")
        .validate_with({
            move |input: &u16| -> Result<(), String> {
                let target = format!("{}:{}", imap_value, input);

                if let Ok(mut addrs) = target.to_socket_addrs() {
                    if let Some(address) = addrs.next() {
                        if TcpStream::connect_timeout(&address, Duration::from_secs(3)).is_ok() {
                            return Ok(());
                        }
                    }
                }
                Err(format!("Could not connect to port {}", input))
            }
        })
        .interact_text()
        .unwrap();

    let mail: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Email")
        .validate_with({
            move |input: &String| -> Result<(), &str> {
                if EmailAddress::is_valid(input) {
                    Ok(())
                } else {
                    Err("Invalid email address")
                }
            }
        })
        .interact_text()
        .unwrap();

    let password = Password::with_theme(&ColorfulTheme::default())
        .with_prompt("Password")
        .validate_with(|input: &String| -> Result<(), String> {
            let tls = native_tls::TlsConnector::builder().build().unwrap();

            let client = if let Ok(client) = imap::connect((imap.clone(), imap_port), &imap, &tls) {
                client
            } else if let Ok(client) =
                imap::connect_starttls((imap.clone(), imap_port), &imap, &tls)
            {
                client
            } else {
                eprintln!("Failed to connect via TLS and STARTTLS");
                keyring_core::unset_default_store();
                process::exit(1);
            };

            match client.login(mail.clone(), input) {
                Ok(_session) => return Ok(()),
                Err((e, _unauth_client)) => return Err(format!("Error: {}", e)),
            };
        })
        .interact()
        .unwrap();

    // the plan here: save password as password to keyring and email and other config to a file
    // somewhere else
    // so keyring would look like: <id here>@ion-mail (i think) and password ****

    let mut new_id: u32 = 0;

    let mut current_active = true;

    for item in &old_config.accounts {
        if item.email == mail
            && item.smtp == smtp
            && item.smtp_port == smtp_port
            && item.imap == imap
            && item.imap_port == imap_port
        {
            eprintln!("Account already exists");
            keyring_core::unset_default_store();
            process::exit(1);
        }
        if item.active {
            current_active = false;
        }
        if item.id >= new_id {
            new_id = item.id + 1;
            if new_id >= (u32::MAX - 10).try_into().unwrap() {
                eprintln!("ID too large");
                keyring_core::unset_default_store();
                process::exit(1);
            }
        }
    }

    let mut _config = old_config;

    let new_account = Account {
        id: new_id,
        email: mail,
        active: current_active,
        smtp: smtp,
        smtp_port: smtp_port,
        imap: imap,
        imap_port: imap_port,
    };

    _config.accounts.push(new_account);

    let toml_output = match toml::to_string(&_config) {
        Ok(toml) => toml,
        Err(e) => {
            eprintln!("Unexpected Error: {}", e);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    };
    match fs::write(toml_path, toml_output) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error when saving new config: {}", e);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    }

    let entry = match Entry::new(APP_NAME, &new_id.to_string()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Unexpected Error: {}", e);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    };
    match entry.set_password(&password) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Failed to set password: {}", e);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    };
}

pub fn list(config: Config) {
    if config.accounts.len() > 0 {
        for account in config.accounts {
            println!(
                "[{id:03}] [{status}] {email}",
                id = account.id,
                status = if account.active { "+" } else { "-" },
                email = account.email
            );
        }
    } else {
        println!("No accounts found");
    }
}

// not use because rust complained
pub fn switch(toml_path: &str, config: Config, account: String) {
    let mut _config = Config {
        accounts: Vec::new(),
    };

    if let Ok(id) = account.parse::<u32>() {
        // loop through all registered, if the current active is found its active flag will be set
        // to false, if the entered id is found its active flag will be set to true
        let mut found = false;
        for mut acc in config.accounts {
            if acc.active {
                acc.active = false;
            }
            if acc.id == id {
                acc.active = true;
                found = true;
            }
            _config.accounts.push(acc);
        }
        if !found {
            eprintln!("Account with ID {} could not be found", id);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    } else {
        // in this case just continue with string
        let mut found = false;
        for mut acc in config.accounts {
            if acc.active {
                acc.active = false;
            }
            if acc.email == account {
                acc.active = true;
                found = true;
            }
            _config.accounts.push(acc);
        }
        if !found {
            eprintln!("Account with email {} could not be found", account);
            process::exit(1);
        }
    }
    let toml_output = match toml::to_string(&_config) {
        Ok(toml) => toml,
        Err(e) => {
            eprintln!("Unexpected Error: {}", e);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    };
    match fs::write(toml_path, toml_output) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error when saving new config: {}", e);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    }

    println!("Successfully switched to account {}", account);
}

pub fn whoami(config: Config) {
    if config.accounts.len() > 0 {
        for account in config.accounts {
            if account.active {
                println!(
                    "{id:03} | {email} | {smtp}:{smtp_port} | {imap}:{imap_port}",
                    id = account.id,
                    email = account.email,
                    smtp = account.smtp,
                    smtp_port = account.smtp_port,
                    imap = account.imap,
                    imap_port = account.imap_port
                );
                return;
            }
        }
        println!("No account is currently active");
    } else {
        println!("No accounts found");
    }
}

pub fn edit(toml_path: &str, old_config: Config, account: Option<String>) {
    let items = vec![
        "Email",
        "Password",
        "SMTP",
        "IMAP",
        "Save & Exit",
        "Save",
        "Exit",
    ];

    let mut config = Config {
        accounts: Vec::new(),
    };

    let mut found = false;

    let mut account_edit = Account {
        id: 0,
        email: "".to_string(),
        active: false,
        smtp: "".to_string(),
        smtp_port: 0,
        imap: "".to_string(),
        imap_port: 0,
    };

    if let Some(ref account) = account {
        if let Ok(id) = account.parse::<u32>() {
            for acc in old_config.accounts {
                if acc.id != id {
                    config.accounts.push(acc);
                } else {
                    account_edit = acc;
                    found = true;
                }
            }

            if !found {
                println!("Account with ID {} could not be found", id);
                keyring_core::unset_default_store();
                process::exit(1);
            }
        } else {
            for acc in old_config.accounts {
                if acc.email != *account {
                    config.accounts.push(acc);
                } else {
                    account_edit = acc;
                    found = true;
                }
            }

            if !found {
                println!("Account with email {} could not be found", account);
                keyring_core::unset_default_store();
                process::exit(1);
            }
        }
    } else {
        for acc in old_config.accounts {
            if !acc.active {
                config.accounts.push(acc);
            } else {
                account_edit = acc;
                found = true;
            }
        }

        if !found {
            println!("No account is currently active");
            keyring_core::unset_default_store();
            process::exit(1);
        }
    }

    let entry = match Entry::new(APP_NAME, &account_edit.id.to_string()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Unexpected Error: {}", e);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    };
    let mut password = match entry.get_password() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to get password: {}", e);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    };

    loop {
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select element to edit")
            .default(0)
            .items(&items)
            .interact()
            .unwrap();

        match selection {
            0 => {
                account_edit.email = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Email")
                    .validate_with({
                        move |input: &String| -> Result<(), &str> {
                            if EmailAddress::is_valid(input) {
                                Ok(())
                            } else {
                                Err("Invalid email address")
                            }
                        }
                    })
                    .interact_text()
                    .unwrap();
            }
            1 => {
                password = Password::with_theme(&ColorfulTheme::default())
                    .with_prompt("Password")
                    .validate_with(|input: &String| -> Result<(), String> {
                        let tls = native_tls::TlsConnector::builder().build().unwrap();

                        let client = if let Ok(client) = imap::connect(
                            (account_edit.imap.clone(), account_edit.imap_port),
                            &account_edit.imap,
                            &tls,
                        ) {
                            client
                        } else if let Ok(client) = imap::connect_starttls(
                            (account_edit.imap.clone(), account_edit.imap_port),
                            &account_edit.imap,
                            &tls,
                        ) {
                            client
                        } else {
                            eprintln!("Failed to connect via TLS and STARTTLS");
                            keyring_core::unset_default_store();
                            process::exit(1);
                        };

                        match client.login(account_edit.email.clone(), input) {
                            Ok(_session) => return Ok(()),
                            Err((e, _unauth_client)) => return Err(format!("Error: {}", e)),
                        };
                    })
                    .interact()
                    .unwrap();
            }
            2 => {
                account_edit.smtp = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("SMTP Server")
                    .validate_with({
                        move |input: &String| -> Result<(), &str> {
                            let target = format!("{}:0", input);

                            match target.to_socket_addrs() {
                                Ok(_) => return Ok(()),
                                Err(_) => Err("Hostname could not be resolved"),
                            }
                        }
                    })
                    .interact_text()
                    .unwrap();

                let smtp_value = account_edit.smtp.clone();
                account_edit.smtp_port = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("SMTP Port")
                    .with_initial_text("587")
                    .validate_with({
                        move |input: &u16| -> Result<(), String> {
                            let target = format!("{}:{}", smtp_value, input);

                            if let Ok(mut addrs) = target.to_socket_addrs() {
                                if let Some(address) = addrs.next() {
                                    if TcpStream::connect_timeout(&address, Duration::from_secs(3))
                                        .is_ok()
                                    {
                                        return Ok(());
                                    }
                                }
                            }
                            Err(format!("Could not connect to port {}", input))
                        }
                    })
                    .interact_text()
                    .unwrap();
            }
            3 => {
                account_edit.imap = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("IMAP Server")
                    .validate_with({
                        move |input: &String| -> Result<(), &str> {
                            let target = format!("{}:0", input);

                            match target.to_socket_addrs() {
                                Ok(_) => return Ok(()),
                                Err(_) => Err("Hostname could not be resolved"),
                            }
                        }
                    })
                    .interact_text()
                    .unwrap();

                let imap_value = account_edit.imap.clone();
                account_edit.imap_port = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("IMAP Port")
                    .with_initial_text("993")
                    .validate_with({
                        move |input: &u16| -> Result<(), String> {
                            let target = format!("{}:{}", imap_value, input);

                            if let Ok(mut addrs) = target.to_socket_addrs() {
                                if let Some(address) = addrs.next() {
                                    if TcpStream::connect_timeout(&address, Duration::from_secs(3))
                                        .is_ok()
                                    {
                                        return Ok(());
                                    }
                                }
                            }
                            Err(format!("Could not connect to port {}", input))
                        }
                    })
                    .interact_text()
                    .unwrap();
            }
            4 => {
                config.accounts.push(account_edit);
                let toml_output = match toml::to_string(&config) {
                    Ok(toml) => toml,
                    Err(e) => {
                        eprintln!("Unexpected Error: {}", e);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    }
                };
                match fs::write(toml_path, toml_output) {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("Error when saving new config: {}", e);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    }
                };

                match entry.set_password(&password.as_str()) {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("Failed to set password: {}", e);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    }
                }
                break;
            }
            5 => {
                config.accounts.push(account_edit.clone());
                let toml_output = match toml::to_string(&config) {
                    Ok(toml) => toml,
                    Err(e) => {
                        eprintln!("Unexpected Error: {}", e);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    }
                };
                match fs::write(toml_path, toml_output) {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("Error when saving new config: {}", e);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    }
                };

                match entry.set_password(&password.as_str()) {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("Failed to set password: {}", e);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    }
                }
            }
            _ => {
                if Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt("Are you sure you want to exit?")
                    .default(true)
                    .show_default(true)
                    .wait_for_newline(true)
                    .interact()
                    .unwrap()
                {
                    break;
                }
            }
        }
    }
}

pub fn logout(toml_path: &str, old_config: Config, account: Option<String>) {
    let mut config = Config {
        accounts: Vec::new(),
    };

    if Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Are you sure you want to logout?")
        .default(true)
        .show_default(true)
        .wait_for_newline(true)
        .interact()
        .unwrap()
    {
        let mut found = false;
        if let Some(account) = account {
            if let Ok(id) = account.parse::<u32>() {
                for acc in old_config.accounts {
                    if acc.id == id {
                        found = true;
                        let entry = match Entry::new(APP_NAME, &id.to_string()) {
                            Ok(e) => e,
                            Err(e) => {
                                eprintln!("Unexpected Error: {}", e);
                                keyring_core::unset_default_store();
                                process::exit(1);
                            }
                        };
                        match entry.delete_credential() {
                            Ok(_) => {}
                            Err(e) => {
                                eprintln!("Error when deleting credential: {}", e);
                                keyring_core::unset_default_store();
                                process::exit(1);
                            }
                        }
                    } else {
                        config.accounts.push(acc);
                    }
                }
            } else {
                for acc in old_config.accounts {
                    if acc.email == account {
                        found = true;
                        let entry = match Entry::new(APP_NAME, &acc.id.to_string()) {
                            Ok(e) => e,
                            Err(e) => {
                                eprintln!("Unexpected Error: {}", e);
                                keyring_core::unset_default_store();
                                process::exit(1);
                            }
                        };
                        match entry.delete_credential() {
                            Ok(_) => {}
                            Err(e) => {
                                eprintln!("Error when deleting credential: {}", e);
                                keyring_core::unset_default_store();
                                process::exit(1);
                            }
                        }
                    } else {
                        config.accounts.push(acc);
                    }
                }
            }
        } else {
            // shift active to some other
            // or easier:
            // user has to switch manually
            for acc in old_config.accounts {
                if acc.active {
                    found = true;
                    let entry = match Entry::new(APP_NAME, &acc.id.to_string()) {
                        Ok(e) => e,
                        Err(e) => {
                            eprintln!("Unexpected Error: {}", e);
                            keyring_core::unset_default_store();
                            process::exit(1);
                        }
                    };
                    match entry.delete_credential() {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("Error when deleting credential: {}", e);
                            keyring_core::unset_default_store();
                            process::exit(1);
                        }
                    }
                } else {
                    config.accounts.push(acc);
                }
            }
            // shift active to first account (because what else should i do, predict what account the
            // user wants?)
            if config.accounts.len() > 0 {
                config.accounts[0].active = true;
            }

            if !found {
                println!("No account is currently active");
            }
        }

        if found {
            let toml_output = match toml::to_string(&config) {
                Ok(toml) => toml,
                Err(e) => {
                    eprintln!("Unexpected Error: {}", e);
                    keyring_core::unset_default_store();
                    process::exit(1);
                }
            };
            match fs::write(toml_path, toml_output) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Error when saving new config: {}", e);
                    keyring_core::unset_default_store();
                    process::exit(1);
                }
            }
            println!("Logout successful");
        }
    }
}
