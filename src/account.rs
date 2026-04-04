use email_address::EmailAddress;
use dialoguer::{theme::ColorfulTheme, Input, Password};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

pub fn add() {
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

                let imap_session = match client.login(mail.clone(), input) {
                    Ok(_session) => return Ok(()),
                    Err((e, _unauth_client)) => return Err(format!("Error: {}", e)),
                };
            } else {
                let client = imap::connect((imap.clone(), imap_port), &imap, &tls).unwrap();

                let imap_session = match client.login(mail.clone(), input) {
                    Ok(_session) => return Ok(()),
                    Err((e, _unauth_client)) => return Err(format!("Error: {}", e)),
                };
            }
        })
        .interact()
        .unwrap();

    println!("smtp server: {}, smtp port: {},  imap server: {}, imap port: {}, mail address: {}, password: {}", smtp, smtp_port, imap, imap_port, mail, password);
}

pub fn list() {
    todo!("Implement listing all users");
}

// not use because rust complained
pub fn switch(account: String) {
    todo!("Implement switching to account {}", account);
}

pub fn whoami() {
    todo!("Implement getting info about current user");
}

pub fn edit(account: Option<String>) {
    if let Some(account) = account {
        todo!("Implement editing account {}", account);
    } else {
        todo!("Implement editing current account");
    }
}

pub fn logout(account: Option<String>) {
    if let Some(account) = account {
        todo!("Implement logging out of account {}", account);
    } else {
        todo!("Implement logging out of current account");
    }
}
