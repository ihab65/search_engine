use std::{ fs::File, io, path::Path, collections::HashMap };
use xml::reader::{ XmlEvent::Characters, EventReader };


#[derive(Debug)]
struct Lexer<'a> {
    content: &'a [char]
}

impl<'a> Lexer<'a> {
    fn new(content: &'a [char]) -> Self {
        Self { content }
    }

    fn trim_left(&mut self) {
        while !self.content.is_empty() && self.content[0].is_whitespace() {
            self.content = &self.content[1..];
        }
    }

    fn chop(&mut self, n: usize) -> &'a [char] {
        let token = &self.content[0..n];
        self.content = &self.content[n..];
        token
    }

    fn chop_while<P>(&mut self, mut predicate: P) -> &'a [char] where P: FnMut(&char) -> bool {
        let mut n = 0;
        while n < self.content.len() && predicate(&self.content[n]) {
            n += 1;
        }
        self.chop(n)
    }

    fn next_token(&mut self) -> Option<&'a [char]> {
        self.trim_left();
        if self.content.is_empty() {
            return None;
        }

        if self.content[0].is_numeric() {
            return Some(self.chop_while(|x: &char| x.is_numeric()))
        }

        if self.content[0].is_alphabetic() {
            return Some(self.chop_while(|x: &char| x.is_alphabetic()))
        }
        return Some(self.chop(1))
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = &'a [char];

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}


fn index_document(doc_content: &str) -> HashMap<String, usize> {
    unimplemented!()
}

fn read_xml_file<P: AsRef<Path>>(file_path: &P) -> io::Result<String> {
    let file = File::open(file_path)?;
    let mut content = String::new();

    for event in EventReader::new(file).into_iter() {
        if let Characters(text) = event.expect("TODO") {
            content.push_str(&text);
            content.push_str(" ");
        }
    }
    Ok(content)
}

fn main() -> io::Result<()> {
    let content: Vec<char> = read_xml_file(&"docs.gl/gl4/glWaitSync.xhtml")?
        .chars()
        .collect::<Vec<_>>();

    let mut tf = HashMap::<String, usize>::new();

    for token in Lexer::new(&content) {
        let term = token.iter().map(|x| x.to_ascii_uppercase()).collect::<String>();
        
        if let Some(freq) = tf.get_mut(&term) {
            *freq += 1;
        } else {
            tf.insert(term, 1);
        };
    }

    let mut stats = tf.iter().collect::<Vec<_>>();
    stats.sort_by_key(|(_, f)| *f);
    stats.reverse();

    for (t, f) in stats.iter().take(10) {
        println!("{t} => {f}")
    }

    /* 
    let dir_path = "docs.gl/gl4";
    let dir = fs::read_dir(dir_path)?;

    for entry in dir {
        let entry = entry?;
        let file_path = entry.path();
        let xml_content = read_xml_file(&file_path);
        
        println!("{file_path:?} => {size}", size = xml_content?.len())
    }
    */

    Ok(())
}
