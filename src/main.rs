#[cfg(target_os = "windows")]
mod game;
#[cfg(target_os = "windows")]
pub mod serverlist {
    include!(concat!(env!("OUT_DIR"), "/tera.rs"));
}

mod util;

use crate::util::*;

use anyhow::{Result, Error};

use serde::{Deserialize, Serialize};
use serde_json::Serializer;
use std::env::args;
use std::fs::{File, exists};
use std::io::Write;
use std::process::exit;
use tokio::try_join;
use ureq::Agent;

const MAX_RETRIES: u8 = 3;

#[tokio::main]
async fn main() -> Result<()> {
	if !exists(get_config_path()?)? {
		eprintln!("Config file does not exist! saving a default one...");
		let config_path = get_config_path()?;
		let mut file = File::create(config_path)?;
		let contents = toml::to_string(&Config::new())?;
		file.write_all(contents.into_bytes().as_ref())?;
		return Ok(());
	}
	let agent = ureq::Agent::new_with_defaults();
	let no_update = args().any(|arg| arg == "--no-update");

	if !exists(get_login_token_path()?)? {
		print!("Login: ");
		let username = read_line()?;
		print!("Password: ");
		let password = read_line()?;
		println!("Logging in...");
		login(&agent, username, password).await?;
	}

	if !no_update {
		println!("Now updating...");
		let mut local_cache = load_cache_from_disk()?;
		if local_cache.is_empty() {
			println!("No local cache found. First run will take some time.");
		}

		let req = agent.get(get_config()?.update);
		let hashes: HashFile = tokio::task::spawn_blocking(move || {
			let mut res = req.call()?;
			res.body_mut().read_json()
		}).await??;

		let my_path = get_my_dir()?;
		let mut index = 1;
		for info in &hashes.files {
			print!("checking {:?}/{:?} {:?}", index, hashes.files.len(), &info.path);
			#[cfg(not(target_os = "windows"))]
			print!("{}\r", termion::clear::AfterCursor);
			#[cfg(target_os = "windows")]
			println!();
			index += 1;
			let target_file = my_path.join(&info.path);
			if let Some(existing) = local_cache.get(&info.path) {
				if existing.eq_ignore_ascii_case(&info.hash) {
					continue;
				}
			} else if exists(&target_file)? {
				let local_hash = calculate_file_hash(&target_file)?;
				if local_hash.eq_ignore_ascii_case(&info.hash) {
					local_cache.insert(info.path.clone(), local_hash);
					continue;
				}
			}

			let parent_path = target_file.parent().unwrap();
			if !exists(parent_path)? {
				std::fs::create_dir_all(parent_path)?;
			}

			println!("Downloading {:?} -> {:?}", info.path, info.hash);

			let mut retries = MAX_RETRIES;
			let res = loop {

				let req = agent.get(info.url.clone());
				let res = tokio::task::spawn_blocking(move || {
					let mut res = req.call()?;
					res.body_mut()
						.with_config()
						.limit(1024 * 1024 * 1024) // for game files
						.read_to_vec()
				}).await?;
				match res {
					Ok(req) => break req,
					Err(e) => {
						if retries < 1 {
							return Err(Error::new(e));
						}
						println!("Download failed, retrying {} more times...", retries);
					}
				}
				retries -= 1;
			};
			let mut file = File::create(target_file)?;
			file.write_all(&res)?;
			local_cache.insert(info.path.clone(), info.hash.clone());
		}

		write_cache_to_disk(local_cache)?;
	}

	#[cfg(target_os = "windows")]
	game::launch(get_my_dir()?.join(get_config()?.path.unwrap_or("Binaries/TERA.exe".to_string()))).await?;

	Ok(())
}

fn login_auth_key(agent: &Agent) -> Result<AuthResponse> {
	let mut res = agent.get(get_config()?.auth).call()?;
	let json: AuthResponse = res.body_mut().read_json()?;

	if !json.return_value {
		eprintln!("Invalid session! {} {}", json.return_code, json.msg);
		exit(1);
	}

	Ok(json)
}

fn create_session(client: &Agent, username: String, password: String) -> Result<()> {
	dbg!(get_config()?);
	let _ = client.post(get_config()?.login).send_form(
		vec![("login", username), ("password", password)]
	)?;
	let cookies = client.cookie_jar_lock();
	cookies.iter().find(|c| c.name().contains("launcher")).unwrap_or_else(|| {
		eprintln!("Failed to log in");
		exit(1);
	});

	Ok(())
}

fn get_account_info(client: &Agent) -> Result<AccountInfoResponse> {
	let mut res = client.get(get_config()?.account).call()?;
	let json: AccountInfoResponse = res.body_mut().read_json()?;

	if !json.return_value {
		eprintln!("Invalid session! {} {}", json.return_code, json.msg);
		exit(1);
	}

	Ok(json)
}

fn get_char_count(client: &Agent) -> Result<CharacterResponse> {
	let mut res = client.get(get_config()?.characters).call()?;
	let json: CharacterResponse = res.body_mut().read_json()?;

	if !json.return_value {
		eprintln!("Invalid session! {} {}", json.return_code, json.msg);
		exit(1);
	}

	Ok(json)
}

async fn login(client: &Agent, username: String, password: String) -> Result<()> {
	let c = client.clone();

	tokio::task::spawn_blocking(|| {
		let c = c;
		create_session(&c, username, password)
	}).await??;
	let (auth, account, characters) = try_join!(
		tokio::task::spawn_blocking({
			let client = client.clone();
			move || login_auth_key(&client)
		}),
		tokio::task::spawn_blocking({
			let client = client.clone();
			move || get_account_info(&client)
		}),
		tokio::task::spawn_blocking({
			let client = client.clone();
			move || get_char_count(&client)
		}),
	)?;
	let auth = auth?;
	let account = account?;
	let characters = characters?;

	let login = LoginResponse {
		return_value: true,
		return_code: 0,
		msg: "success".to_string(),
		character_count: Some(characters.character_count),
		permission: account.permission,
		privilege: account.privilege,
		user_no: account.user_no,
		user_name: account.user_name,
		auth_key: Some(auth.auth_key),
	};
	let token_path = get_login_token_path()?;
	println!("Saving {:?}", token_path);
	dbg!(&login);

	let file = File::create(token_path)?;
	let mut serialize = Serializer::new(file);
	login.serialize(&mut serialize)?;

	Ok(())
}
