use walkdir::WalkDir;

use clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version, App, Arg};

use crossbeam_channel::unbounded;

use std::path::Path;

use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;

const READER_THREADS: usize = 2;
const WRITER_THREADS: usize = 12;

fn parse_passwords(file: &Path) -> Vec<String> {
    let mut res = Vec::new();

    let f = File::open(file).unwrap();
    let f = BufReader::new(f);

    for line in f.lines().filter_map(|x| x.ok()) {
        if let Some(password) = line.split(':').last() {
            res.push(password.to_string());
        }
    }

    res
}

use std::collections::HashMap;
use std::path::PathBuf;

fn alphabet() -> impl Iterator<Item = char> {
    (b'a'..=b'z') // Start as u8
        .map(|c| c as char) // Convert all to chars
        .filter(|c| c.is_alphabetic()) // Filter only alphabetic chars
}

fn create_dir2(root: &Path, c: char) -> PathBuf {
    let path: PathBuf = root.join(c.to_string()).into();

    create_dir(&path);

    path
}

fn create_dir(path: &Path) {
    if !path.exists() {
        std::fs::create_dir(path).unwrap();
    }
}

use std::sync::Arc;
use std::sync::Mutex;

fn gen_file_tree(
    output_dir: &Path,
) -> Arc<HashMap<char, HashMap<char, HashMap<char, Mutex<File>>>>> {
    let mut map = HashMap::new();

    for c1 in alphabet() {
        let dir = create_dir2(output_dir, c1);

        for c2 in alphabet() {
            let dir = create_dir2(&dir, c2);

            for c3 in alphabet() {
                let file = File::create(dir.join(c3.to_string())).unwrap();

                if map.get(&c1).is_none() {
                    map.insert(c1, HashMap::new());
                }

                let map = map.get_mut(&c1).unwrap();

                if map.get(&c2).is_none() {
                    map.insert(c2, HashMap::new());
                }

                let map = map.get_mut(&c2).unwrap();

                map.insert(c3, Mutex::new(file));
            }
        }
    }

    Arc::new(map)
}

fn main() {
    let m = app_from_crate!()
        .arg(
            Arg::with_name("input_dir")
                .required(true)
                .short("i")
                .takes_value(true)
                .value_name("DIR"),
        )
        .arg(
            Arg::with_name("output_dir")
                .required(true)
                .short("o")
                .takes_value(true)
                .value_name("DIR"),
        )
        .get_matches();

    let input_dir = m.value_of("input_dir").unwrap();
    let output_dir = m.value_of("output_dir").unwrap();

    let input_dir = std::path::Path::new(input_dir);
    let output_dir = std::path::Path::new(output_dir);

    if !input_dir.is_dir() {
        eprintln!("input_dir is not a directory: {:?}", input_dir);
        return;
    }

    if !output_dir.is_dir() {
        eprintln!("output_dir is not a directory: {:?}", output_dir);
        return;
    }

    rayon::ThreadPoolBuilder::new()
        .num_threads(READER_THREADS)
        .build_global()
        .unwrap();

    println!("Generating output file structure.");

    let files = gen_file_tree(output_dir);

    let dir_count = WalkDir::new(input_dir)
        .into_iter()
        // Only iterate over valid entries
        .filter_map(|e| e.ok())
        // only iterate over files
        .filter(|e| e.file_type().is_file())
        .fold(0, |acc, _| acc + 1);

    println!("About to iterate over {} password files.", dir_count);

    let (sender, reciever) = unbounded::<Vec<String>>();

    for entry in WalkDir::new(input_dir)
        .into_iter()
        // Only iterate over valid entries
        .filter_map(|e| e.ok())
        // only iterate over files
        .filter(|e| e.file_type().is_file())
    {
        //println!("{:?}", entry.path());

        let sender = sender.clone();

        rayon::spawn(move || {
            let sender = sender.clone();

            loop {
                use sysinfo::SystemExt;

                if sysinfo::System::default().get_free_memory() < 1.2e+7 as u64 {
                    std::thread::sleep(std::time::Duration::from_millis(10000));
                } else {
                    break;
                }
            }

            sender.send(parse_passwords(entry.path())).unwrap();
        });
    }

    use indicatif::{ProgressBar, MultiProgress};
    use rayon::iter::IntoParallelRefIterator;
    use indicatif::ProgressStyle;

    let spinner_style = ProgressStyle::default_spinner()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:80.cyan/blue}] {pos}/{len} ({eta})")
        .progress_chars("#>-");

    let overall_pbar = ProgressBar::new(dir_count);
    overall_pbar.set_style(spinner_style.clone());
    overall_pbar.enable_steady_tick(16);

    let overall_pbar = Arc::new(Mutex::new(overall_pbar));

    let m = MultiProgress::new();

    let mut handles = Vec::new();

    for _ in 0..WRITER_THREADS {
        let files = files.clone();
        let reciever = reciever.clone();
        let overall_pbar = overall_pbar.clone();

        handles.push(std::thread::spawn(move || {
            let files = files.clone();
            let overall_pbar = overall_pbar.clone();

            while let Ok(message) = reciever.recv() {
                for password in message {
                    write_password(&files, &password);
                }

                overall_pbar.lock().unwrap().inc(1);
            }
        }));

        //println!("message {:?}", message.len());
    }

    for handle in handles {
        handle.join().unwrap();
    }

    overall_pbar.lock().unwrap().finish();

    println!("Done!");

    //println!("input_dir {:?}", input_dir);
    //println!("output_dir {:?}", output_dir);
}

fn write_password(
    files: &Arc<HashMap<char, HashMap<char, HashMap<char, Mutex<File>>>>>,
    password: &str,
) {
    if password.chars().count() >= 3 {
        let mut chars = password.chars();

        let c1 = chars.next().expect("not enough chars");
        let c2 = chars.next().expect("not enough chars");
        let c3 = chars.next().expect("not enough chars");

        let c1 = c1
            .to_lowercase()
            .next()
            .expect("could not convert to lowercase");
        let c2 = c2
            .to_lowercase()
            .next()
            .expect("could not convert to lowercase");
        let c3 = c3
            .to_lowercase()
            .next()
            .expect("could not convert to lowercase");

        if c1.is_ascii_alphabetic() && c2.is_ascii_alphabetic() && c3.is_ascii_alphabetic() {
            //println!("password: {}, chars: {}, {}, {}", password, c1, c2, c3);

            let file = files
                .get(&c1)
                .expect(&format!("expected {:?}", c1))
                .get(&c2)
                .expect(&format!("expected {:?}", c2))
                .get(&c3)
                .expect(&format!("expected {:?}", c3));

            let mut file = file.lock().unwrap();

            use std::io::Write;

            writeln!(file, "{}", password);
        }
    }
}
