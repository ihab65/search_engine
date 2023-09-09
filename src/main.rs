use std::{
    env,
    fs::{self, File},
    io::{BufReader, BufWriter},
    path::Path,
    process::ExitCode,
};
use xml::{
    common::{Position, TextPosition},
    reader::{EventReader, XmlEvent},
};

mod model;
mod server;
use model::*;

fn parse_xml_file(file_path: &Path) -> Result<String, ()> {
    let file = File::open(file_path)
        .map_err(|err| eprintln!("ERROR: could not open file {}: {err}", file_path.display()))?;

    let mut content = String::new();

    for event in EventReader::new(BufReader::new(file)).into_iter() {
        let event = event.map_err(|err| {
            let TextPosition { row, column } = err.position();
            let msg = err.msg();
            eprintln!(
                "{file_path}:{row}:{column}: ERROR: {msg}",
                file_path = file_path.display()
            )
        })?;

        if let XmlEvent::Characters(text) = event {
            content.push_str(&text);
            content.push(' ');
        }
    }
    Ok(content)
}

fn tfi_folder(dir_path: &Path, tfi: &mut TermFreqIndex) -> Result<(), ()> {
    let dir = fs::read_dir(dir_path).map_err(|err| {
        eprintln!(
            "ERROR: could not open directory {} for indexing: {err}",
            dir_path.display()
        )
    })?;

    'next_file: for file in dir {
        let file = file.map_err(|err| {
            eprintln!(
                "ERROR: could not read next file in directory {} during indexing: {err}",
                dir_path.display()
            )
        })?;

        let file_path = file.path();

        let dot_file = file_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.starts_with('.'))
            .unwrap_or(false);

        if dot_file {
            continue 'next_file;
        }

        let file_type = file.file_type().map_err(|err| {
            eprintln!(
                "ERROR: could not determine type of file {file_path}: {err}",
                file_path = file_path.display()
            );
        })?;

        if file_type.is_dir() {
            tfi_folder(&file_path, tfi)?;
            continue 'next_file;
        }

        println!("Indexing {:?}", &file_path);

        let content = match parse_xml_file(&file_path) {
            Ok(content) => content.chars().collect::<Vec<_>>(),
            Err(()) => continue 'next_file,
        };

        let mut tf: TermFreq = TermFreq::new();

        for term in Lexer::new(&content) {
            if let Some(freq) = tf.get_mut(&term) {
                *freq += 1;
            } else {
                tf.insert(term, 1);
            }
        }
        let mut stats = tf.iter().collect::<Vec<_>>();
        stats.sort_by_key(|(_, f)| *f);
        stats.reverse();

        tfi.insert(file_path, tf);
    }

    Ok(())
}

fn save_tfi(tfi: &TermFreqIndex, index_path: &str) -> Result<(), ()> {
    println!("Saving {}", index_path);

    let index_file = File::create(index_path)
        .map_err(|err| eprintln!("ERROR: could not create index file {index_path}: {err}"))?;

    serde_json::to_writer_pretty(BufWriter::new(index_file), &tfi).map_err(|err| {
        eprintln!("ERROR: could not serialize index into file {index_path}: {err}")
    })?;

    Ok(())
}

fn usage(program: &str) {
    eprintln!("Usage: {program} [SUBCOMMAND] [OPTIONS]");
    eprintln!("Subcommandes:");
    eprintln!(
        "     index  <folder>                index the <folder> and save the index to index.json"
    );
    eprintln!("     search <index-file> <query>     search <query> within the <index-file>");
    eprintln!("     serve  <index-file> [address]  start the local HTTP server with web interface")
}

fn entry() -> Result<(), ()> {
    let mut args = env::args();
    let program = args.next().expect("path to program is provided");

    let subcommand = args.next().ok_or_else(|| {
        usage(&program);
        eprintln!("ERROR: no subcommand is provided");
    })?;

    match subcommand.as_str() {
        "index" => {
            let dir_path = args.next().ok_or_else(|| {
                usage(&program);
                eprintln!("ERROR: no dir is provided for {subcommand} subcommand")
            })?;

            let mut tfi: TermFreqIndex = TermFreqIndex::new();
            tfi_folder(Path::new(&dir_path), &mut tfi)?;
            save_tfi(&tfi, "index.json")
        }
        "search" => {
            let index_path = args.next().ok_or_else(|| {
                usage(&program);
                eprintln!("ERROR: no path to index is provided for {subcommand} subcommand")
            })?;

            let prompt = args
                .next()
                .ok_or_else(|| {
                    usage(&program);
                    eprintln!("ERROR: no search query is provided {subcommand} subcommand");
                })?
                .chars()
                .collect::<Vec<_>>();

            let index_file = File::open(&index_path).map_err(|err| {
                eprintln!("ERROR: could not open index file {index_path}: {err}");
            })?;

            let tf_index: TermFreqIndex = serde_json::from_reader(BufReader::new(index_file))
                .map_err(|err| {
                    eprintln!("ERROR: could not parse index file {index_path}: {err}");
                })?;

            for (path, rank) in search_query(&tf_index, &prompt).iter().take(20) {
                println!("{path} {rank}", path = path.display());
            }

            Ok(())
        }
        "serve" => {
            let index_path = args.next().ok_or_else(|| {
                usage(&program);
                eprintln!("ERROR: no path to index is provided for {subcommand} subcommand")
            })?;

            let index_file = File::open(&index_path).map_err(|err| {
                eprintln!("ERROR: could not open index file {}: {err}", index_path)
            })?;

            let tf_index: TermFreqIndex = serde_json::from_reader(index_file).map_err(|err| {
                eprintln!("ERROR: could not parse index file {index_path}: {err}")
            })?;

            let address: String = args.next().unwrap_or("127.0.0.1:6969".to_string());
            server::start(&address, &tf_index)
        }
        _ => {
            usage(&program);
            eprintln!("ERROR: unkown subcommand {subcommand}");
            Err(())
        }
    }
}

fn main() -> ExitCode {
    match entry() {
        Ok(()) => ExitCode::SUCCESS,
        Err(()) => ExitCode::FAILURE,
    }
}
