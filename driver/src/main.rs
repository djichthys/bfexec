use std::env; 


extern crate interpreter; 
const HEAPSIZE: usize = 2 * 1024; 

mod parser {
    pub fn new_program(bytestream: &[u8], heapsz: usize) -> interpreter::Program_State { 
        interpreter::Program_State::new(bytestream, heapsz) 
    }
}

fn main() -> std::io::Result<()> {
    if let Some(prog_name) = env::args().peekable().peek() { 
        println!("program name -> {}", prog_name);
        println!("================");
    }

    for (itr, arg) in env::args().skip(1).enumerate() {
        //println!("Arg[{}] -> {}", itr, arg);
        let buffer = std::fs::read(&arg)?; 
        let mut prog = parser::new_program(&buffer, HEAPSIZE);
        //println!("buffer = {:?}", prog.txt);

        /* Interpret the program */
        if let Ok(ret) = prog.interpret() {
            println!("\n============");
            println!("interpreter returned {}", ret);
            println!("=================");
        }; 
    }
    Ok(())
}
