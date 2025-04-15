use std::env; 

extern crate interpreter; 
use interpreter::interpreter as interp;
mod utils {
    pub fn read_file(file_name: &str) -> Result<Vec<u8>> { 

    }
}



fn main() {
    if let Some(prog_name) = env::args().peekable().peek() { 
        println!("program name -> {}", prog_name);
    }

    for (itr, arg) in env::args().skip(1).enumerate() {
        println!("Arg[{}] -> {}", itr, arg);
        if let Ok(ret) = interp::interpret(&arg) { 
            println!("interpreter returned {}" , ret);
        }; 
    }
}
