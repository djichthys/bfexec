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
                    //dbg!(prog.profile);
                    println!("profile"); 
                    println!(" +: {}", prog.profile.arith); 
                    println!(" >: {}", prog.profile.mv); 
                    println!(" ,: {}", prog.profile.inp); 
                    println!(" .: {}", prog.profile.out); 
                    println!(" [: {}", prog.profile.jmp); 
                    println!(" ]: {}", prog.profile.ret); 
                    println!(" loops:");
                    let repr = |range: std::ops::Range<usize>| -> String { 
                        prog.txt[range]
                            .iter()
                            .map( |x| match x { 
                                interpreter::BF_ISA::Incr(n) => { 
                                    if *n >= 128 { 
                                        format!("-{}", n.wrapping_neg())
                                    } else { 
                                        format!("+{}", n)
                                    }
                                }, 
                                interpreter::BF_ISA::Mv(n) => { 
                                    if *n < 0 {
                                        format!("<{}", -n)
                                    } else { 
                                        format!(">{}", n) 
                                    }
                                },
                                interpreter::BF_ISA::In => ",".to_string(),
                                interpreter::BF_ISA::Out => ".".to_string(),
                                interpreter::BF_ISA::Jmp(_) => "[".to_string(),
                                interpreter::BF_ISA::Ret(_) => "]".to_string(),
                            })
                            .fold(String::new(), |a,b| a + &b)
                    };

                    let mut loops: Vec<_> = prog.profile
                        .loops
                        .into_iter()
                        .map(|(range, count)| (repr(range), count))
                        .collect(); 

                    loops.sort_by(|a,b| a.0.cmp(&b.0));
                    for idx in 1..loops.len() { 
                        if loops[idx - 1].0 == loops[idx].0 { 
                            loops[idx].1 + loops[idx-1].1;
                            loops[idx-1].1 = 0; /* mark to remove */
                        }
                    }
                    loops.retain(|x| x.1 > 0);
                    loops.sort_by_key(|x| x.1);
                    for (code, count) in loops.into_iter().rev().take(20) { 
                        println!("{:10}: {}", count, code); 
                    }
                }
            }; 
            /* Interpret the program */
            //println!("buffer = {:?}", prog.txt);
        }; 
    }
    Ok(())
}
