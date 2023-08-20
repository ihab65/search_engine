use std::{
    fs::{File, self},
    io, path::Path
};
use xml::reader::{
    XmlEvent::Characters,
    EventReader
};

fn read_xml_file<P: AsRef<Path>>(file_path: &P) -> io::Result<String> {
    let file = File::open(file_path)?;
    let mut content = String::new();

    for event in EventReader::new(file).into_iter() {
        if let Characters(text) = event.expect("TODO") {
            content.push_str(&text);
            
        }
    }
    Ok(content)
}

fn main() -> io::Result<()> {
    let dir_path = "docs.gl/gl4";
    let dir = fs::read_dir(dir_path)?;

    for entry in dir {
        let entry = entry?;
        let file_path = entry.path();
        let xml_content = read_xml_file(&file_path);
        
        println!("{file_path:?} => {size}", size = xml_content?.len())
    }

    Ok(())
}
