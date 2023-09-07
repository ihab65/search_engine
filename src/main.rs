use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    ops::RangeBounds,
    path::{Path, PathBuf},
    process::ExitCode,
};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use xml::{
    common::{Position, TextPosition},
    reader::{EventReader, XmlEvent},
};

mod lexer;

type TF = HashMap<String, usize>;
type TFI = HashMap<PathBuf, TF>;

fn parse_xml_file(file_path: &Path) -> Result<String, ()> {
    let file = File::open(file_path)
        .map_err(|err| eprintln!("ERROR: could not open file {}: {err}", file_path.display()))?;

    let mut content = String::new();

    for event in EventReader::new(file).into_iter() {
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

fn tfi_folder(dir_path: &Path, tfi: &mut TFI) -> Result<(), ()> {
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

        let mut tf: TF = TF::new();

        for term in lexer::Lexer::new(&content) {
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

fn save_tfi(tfi: &TFI, index_path: &str) -> Result<(), ()> {
    println!("Saving {}", index_path);

    let index_file = File::create(index_path)
        .map_err(|err| eprintln!("ERROR: could not create index file {index_path}: {err}"))?;

    serde_json::to_writer_pretty(index_file, &tfi).map_err(|err| {
        eprintln!("ERROR: could not serialize index into file {index_path}: {err}")
    })?;

    Ok(())
}

fn check_index(index_path: &str) -> Result<(), ()> {
    println!("Reading {} index file", index_path);

    let index_file = File::open(index_path)
        .map_err(|err| eprintln!("ERROR: could not open index file {index_path}: {err}"))?;

    let tfi: TFI = serde_json::from_reader(index_file)
        .map_err(|err| eprintln!("ERROR: could not parse index file {index_path}: {err}"))?;

    println!("{index_path} contains {count} files", count = tfi.len());

    Ok(())
}

fn usage(program: &str) {
    eprintln!("Usage: {program} [SUBCOMMAND] [OPTIONS]");
    eprintln!("Subcommandes:");
    eprintln!(
        "     index  <folder>                index the <folder> and save the index to index.json"
    );
    eprintln!("     search <index-file>            check how many documents are indexed in the file (search not implemented yet)");
    eprintln!("     serve  <index-file> [address]  start the local HTTP server with web interface")
}

fn serve_404(request: Request) -> Result<(), ()> {
    request
        .respond(Response::from_string("404").with_status_code(StatusCode(404)))
        .map_err(|err| eprintln!("ERROR: could not serve a request: {err}"))
}

fn serve_static_file(request: Request, file_path: &str, content_type: &str) -> Result<(), ()> {
    let content_type = Header::from_bytes("Content-Type", content_type).expect("failed");
    let js = File::open(file_path)
        .map_err(|err| eprintln!("ERROR: could not serve file {file_path}: {err}"))?;

    let response = Response::from_file(js).with_header(content_type);
    request
        .respond(response)
        .map_err(|err| eprintln!("ERROR: could not serve a request: {err}"))
}

fn tf(t: &str, d: &TF) -> f32 {
    // t stands for "term" and d for "document"
    let a = *d.get(t).unwrap_or(&0) as f32;
    let b = d.iter().map(|(_, f)| *f).sum::<usize>() as f32;
    a / b
}

fn idf(t: &str, d: &TFI) -> f32 {
    let n = d.len() as f32;
    let m = d.values().filter(|tf| tf.contains_key(t)).count() as f32 + 1f32;
    (n / m).log10()
}

fn serve_request(tfi: &TFI, mut request: Request) -> Result<(), ()> {
    println!(
        "INFO: received request! method: {:?}, url: {:?}",
        request.method(),
        request.url()
    );
    match (request.method(), request.url()) {
        (Method::Post, "/api/search") => {
            let mut buf = Vec::new();
            request.as_reader().read_to_end(&mut buf);
            let body = std::str::from_utf8(&buf)
                .map_err(|err| eprintln!("ERROR: could not interpret body as UTF-8 string: {err}"))?
                .chars()
                .collect::<Vec<_>>();

            let mut result = Vec::<(&PathBuf, f32)>::new();
            for (path, tf_table) in tfi {
                let mut rank = 0f32;
                for token in lexer::Lexer::new(&body) {
                    rank += tf(&token, &tf_table) * idf(&token, &tfi);
                }
                result.push((path, rank))
            }

            result.sort_by(|(_, f1), (_, f2)| f1.total_cmp(f2));
            result.reverse();
            
            for (p, f) in result.iter() {
                println!("{:?} => {f}", p.file_name().unwrap());
            }

            request
                .respond(Response::from_string("ok"))
                .map_err(|err| eprintln!("ERROR: {err}"))
        }

        (Method::Get, "/index.js") => {
            serve_static_file(request, "src/index.js", "text/javascript; charset=utf-8")
        }

        (Method::Get, "/") | (Method::Get, "/index.html") => {
            serve_static_file(request, "src/index.html", "text/html; charset=utf-8")
        }

        _ => serve_404(request),
    }
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

            let mut tfi: TFI = TFI::new();
            tfi_folder(Path::new(&dir_path), &mut tfi)?;
            save_tfi(&tfi, "index.json")?;
        }
        "search" => {
            let index_path = args.next().ok_or_else(|| {
                usage(&program);
                eprintln!("ERROR: no path to index is provided for {subcommand} subcommand")
            })?;

            check_index(&index_path)?;
        }
        "serve" => {
            let index_path = args.next().ok_or_else(|| {
                usage(&program);
                eprintln!("ERROR: no path to index is provided for {subcommand} subcommand")
            })?;

            let index_file = File::open(&index_path).map_err(|err| {
                eprintln!("ERROR: could not open index file {}: {err}", index_path)
            })?;

            let tfi: TFI = serde_json::from_reader(index_file).map_err(|err| {
                eprintln!("ERROR: could not parse index file {index_path}: {err}")
            })?;

            let address: String = args.next().unwrap_or("127.0.0.1:6969".to_string());
            let server = Server::http(&address).map_err(|err| {
                eprintln!("ERROR: could not start HTTP server at {address}: {err}")
            })?;

            eprintln!("listening to http://{address}/");

            for request in server.incoming_requests() {
                serve_request(&tfi, request)?;
            }
        }
        _ => {
            usage(&program);
            eprintln!("ERROR: unkown subcommand {subcommand}");
            return Err(());
        }
    }

    Ok(())
}

fn main() -> ExitCode {
    match entry() {
        Ok(()) => ExitCode::SUCCESS,
        Err(()) => ExitCode::FAILURE,
    }
}
