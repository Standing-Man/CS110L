// Simple Hangman Program
// User gets five incorrect guesses
// Word chosen randomly from words.txt
// Inspiration from: https://doc.rust-lang.org/book/ch02-00-guessing-game-tutorial.html
// This assignment will introduce you to some fundamental syntax in Rust:
// - variable declaration
// - string manipulation
// - conditional statements
// - loops
// - vectors
// - files
// - user input
// We've tried to limit/hide Rust's quirks since we'll discuss those details
// more in depth in the coming lectures.
extern crate rand;
use rand::Rng;
use std::fs;
use std::io;
use std::io::Write;
// use std::ops::Index;

const NUM_INCORRECT_GUESSES: u32 = 5;
const WORDS_PATH: &str = "words.txt";

fn pick_a_random_word() -> String {
    let file_string = fs::read_to_string(WORDS_PATH).expect("Unable to read file.");
    let words: Vec<&str> = file_string.split('\n').collect();
    String::from(words[rand::thread_rng().gen_range(0, words.len())].trim())
}

// covert Vector of char to String
fn to_string(vec:&Vec<char>) -> String {
    return vec.iter().collect::<String>();
}

fn index(vec:&Vec<char>, c:char, index_set: &mut Vec<usize>) -> (bool, usize){
    for index in 0..vec.len() {
        if vec[index] == c {
            if index_set.contains(&index) {
                continue;
            } else {
                index_set.push(index);
                return (true, index);
            }
        }
    }
    return (false, vec.len());
}


fn main() {
    let secret_word = pick_a_random_word();
    // Note: given what you know about Rust so far, it's easier to pull characters out of a
    // vector than it is to pull them out of a string. You can get the ith character of
    // secret_word by doing secret_word_chars[i].
    let secret_word_chars: Vec<char> = secret_word.chars().collect();
    // Uncomment for debugging:
    println!("random word: {}", secret_word);

    // Your code here! :)
    println!("Welcome to CS110L Hangman!");

    // 剩余猜想的次数
    let mut error_counter = 0;
    // 猜想正确的字符串
    let mut guessed_words: Vec<char> = vec!['-';secret_word_chars.len()];
    // 已猜想的字符集合
    let mut guessed_letters: Vec<char> = Vec::new();
    // 记录已经成功猜对字符的索引
    let mut index_set: Vec<usize> = Vec::new();

    loop {
        println!("The word so far is {}", to_string(&guessed_words));
        println!("You have guessed the following letters: {}", to_string(&guessed_letters));
        println!("You have {} guesses left", NUM_INCORRECT_GUESSES - error_counter);

        print!("Please guess a letter: ");
        // Make sure the prompt from the previous line gets displayed:
        io::stdout()
            .flush()
            .expect("Error flushing stdout.");
        // 猜想的字符串
        let mut guess = String::new();
        io::stdin()
            .read_line(&mut guess)
            .expect("Error reading line.");
        let guess_as_char = guess.chars().next().unwrap();
        // println!("guess_as_char: {}", guess_as_char);

        let (ok, index) = index(&secret_word_chars, guess_as_char, &mut index_set);
        // println!("ok: {}, index: {}", ok, index);
        guessed_letters.push(guess_as_char);

        if ok {
            guessed_words[index] = guess_as_char;
        } else {
            println!("Sorry, that letter is not in the word");
            error_counter += 1;
        }

        if to_string(&guessed_words) == secret_word {
            println!("Congratulations you guessed the secret word: {}!", secret_word);
            break;
        } else if error_counter == NUM_INCORRECT_GUESSES {
            println!("Sorry, you ran out of guesses!");
            break;
        }
        print!("\n\n\n")
    }
    
}
