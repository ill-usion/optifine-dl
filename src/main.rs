mod constants;

use colored::*;
use constants::*;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use scraper::{Html, Selector};
use std::{
    borrow::BorrowMut,
    env,
    error::Error,
    io::{stdin, stdout, Write},
    path::PathBuf,
};
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
    process::Command,
};

#[derive(Debug, Default)]
struct OptifineVersion {
    filename: String,
    mirror_url: String,
}

impl OptifineVersion {
    async fn get_download_url(&self) -> Result<String, Box<dyn Error>> {
        let doc = download_page(&self.mirror_url).await?;
        let download_anchor_selector = Selector::parse(selectors::DOWNLOAD_ANCHOR).unwrap();
        if let Some(download_anchor) = doc.select(&download_anchor_selector).next() {
            if let Some(download_url) = download_anchor.attr("href") {
                if !download_url.starts_with(OPTIFINE_ENDPOINT) {
                    let ret = String::from(OPTIFINE_ENDPOINT);
                    return Ok(ret + "/" + download_url);
                }
                return Ok(download_url.to_string());
            }
        }

        Err("Download URL not found in page".into())
    }
}

#[derive(Debug, Default)]
struct MinecraftVersion {
    version: String,
    downloads: Vec<OptifineVersion>,
}

const OPTIFINE_SCRAPER_VERSION: &str = "1.0.0";

async fn download_page(url: &str) -> Result<Html, Box<dyn Error>> {
    let resp = reqwest::get(url).await.unwrap();
    let page_content = resp.text().await?;
    let doc = Html::parse_document(&page_content);

    Ok(doc)
}

fn read_version() -> String {
    print!("Enter a Minecraft version ({}): ", "e.g 1.16.5".cyan());
    stdout().flush().unwrap();

    let mut input = String::new();
    stdin().read_line(&mut input).unwrap();

    input.trim().to_owned()
}

fn print_available_downloads(version: &MinecraftVersion) {
    println!(
        "Available {} Optifine downloads ({})",
        version.version.green(),
        version.downloads.len()
    );

    println!(
        "[{}]\t[{}]\t[{}]",
        "index".yellow(),
        "filename".bold().blue(),
        "download url".white()
    );

    for (idx, ver) in version.downloads.iter().enumerate() {
        println!(
            "[{}] {} ({})",
            (idx + 1).to_string().yellow(),
            ver.filename.bold().blue(),
            ver.mirror_url
        );
    }
}

fn select_optifine_version(version: &MinecraftVersion) -> Option<&OptifineVersion> {
    print!("Enter the index of the version you want to download: ");
    stdout().flush().unwrap();

    let mut input = String::new();
    stdin().read_line(&mut input).unwrap();

    let index: usize = input.trim().parse().unwrap_or(version.downloads.len());

    version.downloads.get((index).max(1) - 1)
}

async fn download_of_version(version: &OptifineVersion) -> Result<PathBuf, Box<dyn Error>> {
    let pb = ProgressBar::new(0);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{bar:40}] {percent}% ({bytes}/{total_bytes})")
            .unwrap()
            .progress_chars("#>-"),
    );

    let download_url = version.get_download_url().await?;
    let response = reqwest::get(download_url).await?;

    let total_size = response.content_length().unwrap_or(0);
    pb.set_length(total_size);

    let mut path = env::temp_dir().join(&version.filename);
    path.set_extension("jar");

    let mut file = File::create(&path).await?;
    let mut downloaded_bytes_len = 0;
    let mut file_stream = response.bytes_stream();

    while let Some(chunk) = file_stream.next().await {
        let bytes: &[u8] = &chunk?;

        file.write_all(bytes).await?;
        downloaded_bytes_len += bytes.len() as u64;

        pb.set_position(downloaded_bytes_len);
    }

    pb.finish();
    Ok(path)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("{}", ASCII_LOGO.red());
    println!("Github: {}", "ill-usion".yellow());
    println!("Version: {}", OPTIFINE_SCRAPER_VERSION.yellow());
    println!();

    println!("Downloading {}...", OPTIFINE_DOWNLOADS_ENDPOINT);
    let doc = download_page(OPTIFINE_DOWNLOADS_ENDPOINT).await?;

    let version_header_selector = Selector::parse(selectors::VERSION_HEADER)?;
    let download_row_selector = Selector::parse(selectors::DOWNLOAD_ROW)?;
    let download_filename_selector = Selector::parse(selectors::DOWNLOAD_FILENAME)?;
    let download_mirror_selector = Selector::parse(selectors::DOWNLOAD_MIRROR)?;

    println!("Parsing versions...");
    let mut mc_versions: Vec<MinecraftVersion> = vec![];
    for header in doc.select(&version_header_selector) {
        let content: String = header.text().collect();

        let (_, version_num) = content.split_once(' ').unwrap();
        let version = MinecraftVersion {
            version: version_num.to_owned(),
            ..Default::default()
        };

        mc_versions.push(version);
    }

    let mut version_idx = 0usize;
    for row in doc.select(&download_row_selector) {
        let mut version = mc_versions[version_idx].borrow_mut();

        if let Some(mirror_link) = row.select(&download_mirror_selector).next() {
            let mirror_link = mirror_link.attr("href").unwrap();
            while !mirror_link.contains(&version.version) {
                // bad idea
                version_idx += 1;
                version = mc_versions[version_idx].borrow_mut();
            }

            let mut of_version = OptifineVersion {
                mirror_url: mirror_link.to_owned(),
                ..Default::default()
            };

            if let Some(filename) = row.select(&download_filename_selector).next() {
                let filename: String = filename.text().collect();
                of_version.filename = filename;
            }

            version.downloads.push(of_version);
        }
    }

    println!("{}", "Done.".green());
    let mut target_version: Option<&MinecraftVersion> = None;
    'outer: while target_version.is_none() {
        let version_choice = read_version();
        println!("Searching for version {}...", version_choice);

        for ver in mc_versions.iter() {
            if ver.version == version_choice {
                target_version = Some(ver);
                break 'outer;
            }
        }

        println!("{}", "Version not found.".red());
    }

    let target_version = target_version.unwrap();
    print_available_downloads(target_version);

    let mut of_version: Option<&OptifineVersion> = None;
    while of_version.is_none() {
        if let Some(choice) = select_optifine_version(target_version) {
            of_version = Some(choice);
        } else {
            println!("{}", "Invalid version index".red());
        }
    }

    let of_version = of_version.unwrap();

    println!("Downloading {}...", of_version.filename.bold().blue());
    let jar_file_path = download_of_version(of_version).await?;

    let mut cmd = if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg("start").arg("").arg(&jar_file_path);
        cmd
    } else {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(&jar_file_path);
        cmd
    };

    let _ = cmd.spawn()?;

    println!("Proceed with the installation. When you're done press Enter to exit.");
    stdin().read_line(&mut String::new())?;

    fs::remove_file(jar_file_path).await?;

    Ok(())
}
