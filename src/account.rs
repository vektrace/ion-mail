use crate::{APP_NAME, Config, Account};

use email_address::EmailAddress;
use dialoguer::{theme::ColorfulTheme, Input, Password};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;
use std::fs;
use std::process;
use keyring::Entry;

pub fn add(toml_path: &str, old_config: Config) {
    if old_config.accounts.len() >= (u32::MAX - 10).try_into().unwrap() {
        eprintln!("Too many accounts registered, remove some then try again");
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
            if imap_port == 143 {
                let client = imap::connect_starttls((imap.clone(), 143), &imap, &tls).unwrap();

                let _imap_session = match client.login(mail.clone(), input) {
                    Ok(_session) => return Ok(()),
                    Err((e, _unauth_client)) => return Err(format!("Error: {}", e)),
                };
            } else {
                let client = imap::connect((imap.clone(), imap_port), &imap, &tls).unwrap();

                let _imap_session = match client.login(mail.clone(), input) {
                    Ok(_session) => return Ok(()),
                    Err((e, _unauth_client)) => return Err(format!("Error: {}", e)),
                };
            }
        })
        .interact()
        .unwrap();

    // the plan here: save password as password to keyring and email and other config to a file
    // somewhere else
    // so keyring would look like: <id here>@ion-mail (i think) and password ****
    
    let mut new_id: u32 = 0;

    let mut current_active = true;
    
    for item in &old_config.accounts {
        if item.email == mail && item.smtp == smtp && item.smtp_port == smtp_port && item.imap == imap && item.imap_port == imap_port {
            eprintln!("Account already exists");
            process::exit(1);
        }
        if item.active {
            current_active = false;
        }
        if item.id >= new_id {
            new_id = item.id + 1;
            if new_id >= (u32::MAX - 10).try_into().unwrap() {
                eprintln!("ID to large");
                process::exit(1);
            }
        }
    }

    let mut _config = Config {
        accounts: Vec::new(),
    };

    _config = old_config;

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

    let toml_output = toml::to_string(&_config).expect("Something went wrong");
    fs::write(toml_path, toml_output).expect("Failed to save file");

    let entry = Entry::new(APP_NAME, &new_id.to_string()).unwrap();
    let _ = entry.set_password(&password);
}

pub fn list(config: Config) {
    if config.accounts.len() > 0 {
        for account in config.accounts {
            println!("[{id:03}] [{status}] {email}", id=account.id, status=if account.active { "+" } else { "-" }, email=account.email);
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
    let toml_output = toml::to_string(&_config).expect("Something went wrong");
    fs::write(toml_path, toml_output).expect("Failed to save file");

    println!("Successfully switched to account {}", account);
}

pub fn whoami(config: Config) {
    if config.accounts.len() > 0 {
        for account in config.accounts {
            if account.active {
                println!("{id:03} | {email} | {smtp}:{smtp_port} | {imap}:{imap_port}", id=account.id, email=account.email, smtp=account.smtp, smtp_port=account.smtp_port, imap=account.imap, imap_port=account.imap_port);
                return;
            }
        }
        println!("No account is currently active");
    } else {
        println!("No accounts found");
    }
}

pub fn edit(toml_path: &str, account: Option<String>) {
    if let Some(account) = account {
        todo!("Implement editing account {}", account);
    } else {
        todo!("Implement editing current account");
    }
}

pub fn logout(toml_path: &str, old_config: Config, account: Option<String>) {
    let mut config = Config {
        accounts: Vec::new(),
    };

    let mut found = false;
    if let Some(account) = account {
        if let Ok(id) = account.parse::<u32>() {
            for acc in old_config.accounts {
                if acc.id == id {
                    found = true;
                    let entry = Entry::new(APP_NAME, &id.to_string()).unwrap();
                    let _ = entry.delete_credential();
                } else {
                    config.accounts.push(acc);
                }
            }
        } else {
            for acc in old_config.accounts {
                if acc.email == account {
                    found = true;
                    let entry = Entry::new(APP_NAME, &acc.id.to_string()).unwrap();
                    let _ = entry.delete_credential();
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
                let entry = Entry::new(APP_NAME, &acc.id.to_string()).unwrap();
                let _ = entry.delete_credential();
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
        let toml_output = toml::to_string(&config).expect("Something went wrong");
        fs::write(toml_path, toml_output).expect("Failed to save file");
        println!("Logout successful");
    }
}
