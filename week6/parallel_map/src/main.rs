use crossbeam_channel;
use std::{thread, time};

fn parallel_map<T, U, F>(mut input_vec: Vec<T>, num_threads: usize, f: F) -> Vec<U>
where
    F: FnOnce(T) -> U + Send + Copy + 'static,
    T: Send + 'static,
    U: Send + 'static + Default + std::fmt::Debug,
{
    let mut output_vec: Vec<U> = Vec::with_capacity(input_vec.len());
    for _ in 0..input_vec.len() {
        output_vec.push(U::default());
    }
    let (input_tx, input_rx) = crossbeam_channel::unbounded();
    let (output_tx, output_rx) = crossbeam_channel::unbounded();
    let mut threads = Vec::new();

    for _ in 0..num_threads {
        let input_rx = input_rx.clone();
        let output_tx = output_tx.clone();
        threads.push(thread::spawn(move|| {
            while let Ok((index, num)) = input_rx.recv() {
                output_tx
                    .send((index, f(num)))
                    .expect("Tried to write to the result into the channel");   
            }
        }));
    }

    for (index, num) in input_vec.into_iter().enumerate() {
        input_tx.send((index, num)).expect("Tried to write num from input into the channel");
    }

    drop(input_tx);
    for thread in threads {
        thread.join().expect("Panic occurred in thread");
    }

    drop(output_tx);
    while let Ok(result) = output_rx.recv() {
        output_vec[result.0] = result.1;
    }

    output_vec
}

fn main() {
    let v = vec![6, 7, 8, 9, 10, 1, 2, 3, 4, 5, 12, 18, 11, 5, 20];
    let squares = parallel_map(v, 10, |num| {
        println!("{} squared is {}", num, num * num);
        thread::sleep(time::Duration::from_millis(500));
        num * num
    });
    println!("squares: {:?}", squares);
}
