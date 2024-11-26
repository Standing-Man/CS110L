use std::env;
use std::fs::File; // For read_file_lines()
use std::io::{self, BufRead}; // For read_file_lines()
use std::process;

// read the lines from filename
fn read_file_lines(filename: &String) -> Result<Vec<String>, io::Error> {
    // Be sure to delete the #[allow(unused)] line above
    let mut res: Vec<String> = Vec::new();
    let file = match File::open(filename) {
        Ok(file) => file,
        Err(err) => return Err(err),
    };
    for line in io::BufReader::new(file).lines() {
        let line_str = line?;
        res.push(line_str);
    }
    Ok(res)
}

// counter the lines
fn counter_lines(v: &Vec<String>) -> usize {
    return v.len();
}

// counter the number of words
fn counter_words(lines: &Vec<String>) -> usize {
    let mut words_counter = 0;
    for line in lines {
        let words: Vec<&str> = line.split(' ').collect();
        words_counter += words.len();
    }
    return words_counter;
}

fn counter_bytes(words: &Vec<String>) -> usize {
    let mut n = 0;
    for word in words {
        n += word.len();
    }
    n += words.len();
    n
}


fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Too few arguments.");
        process::exit(1);
    }
    let filename = &args[1];
    // Your code here :)
    let lines = read_file_lines(filename).unwrap();
    let line_counter = counter_lines(&lines);
    let words_counter = counter_words(&lines);
    let byte_counter = counter_bytes(&lines);
    println!("行数 单词数 字节数");
    println!("{}    {}      {}      {}", line_counter, words_counter, byte_counter, filename); 
}
