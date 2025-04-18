use std::env; 


extern crate interpreter; 

const HEAPSIZE: usize = 2 * 1024; 

type ProgType = Result<interpreter::Program_State, interpreter::NestingErr>;

mod parser {
    pub fn new_program(bytestream: &[u8], heapsz: usize) -> super::ProgType {
        interpreter::Program_State::new(bytestream, super::HEAPSIZE)
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
        if let Ok(mut prog) = parser::new_program(&buffer, HEAPSIZE) { 
            if let Ok(ret) = prog.interpret() { 
                println!("\n============"); 
                println!("interpreter returned {}", ret); 
                println!("================="); 

                #[cfg(feature="profile")]
                { 
                    dbg!(prog.profile);
                }
            }; 
            /* Interpret the program */
            //println!("buffer = {:?}", prog.txt);
        }; 
    }
    Ok(())
}
