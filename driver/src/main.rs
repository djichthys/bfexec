extern crate getopts;
extern crate execute; 

use std::env; 

const HEAPSIZE: usize = 2 * 1024; 
type ProgType = Result<execute::ProgramState, execute::NestingErr>;

#[derive(Debug,Default)]
enum ExecutionEngine { 
    #[default] 
    Interpreter,
    CraneLift,
}

mod parser {
    use std::process; 

    pub fn new_program(bytestream: &[u8], heapsz: usize) -> super::ProgType {
        execute::ProgramState::new(bytestream, heapsz)
    }

    #[derive(Debug)]
    pub struct CmdLine {
        pub exec_engine: super::ExecutionEngine,
        pub programs: Vec<String>,
        pub clir: bool,
    }

    impl  CmdLine { 
        fn new(engine: super::ExecutionEngine, programs: Vec<String>) -> Self {
            CmdLine { 
                exec_engine: engine, 
                programs: programs.clone(),
                clir: false,
            }
        }
    } 

    pub fn usage(prog: &str) {
        println!("Usage: {} [-e <Interpreter/CraneLift>  [list of BF programs]", prog); 
    }

    pub fn parse_cmdline<'a,'b>(program_name: &'a str, args: &Vec<String>) -> Result<CmdLine,getopts::Fail> { 
        let mut opts = getopts::Options::new(); 

        opts.parsing_style(getopts::ParsingStyle::FloatingFrees); 
        opts.optflag("h", "help", "Show this menu"); 
        opts.optflag("v", "verbose", "Displays generated cranelift IR"); 
        opts.optopt("e", "exec-env", "jit vs interpret", "<Interpreter/CraneLift>"); 

        let arg_match = opts.parse(&args[1..])?; 
        if arg_match.opt_present("h") {
            usage(program_name);
            process::exit(0);
        } 


        let exec_env = match arg_match.opt_str("e") { 
            Some(v) if v == "Interpreter" => { 
                super::ExecutionEngine::Interpreter
            }, 
            Some(v) if v == "CraneLift" => {
                super::ExecutionEngine::CraneLift 
            }, 
            Some(_) | None => {
                usage(program_name);
                process::exit(-1);
            },
        };

        if arg_match.free.is_empty() { 
            println!("BF program files not provided");
            usage(program_name); 
            process::exit(-1);
        }
            
        Ok(CmdLine { 
            exec_engine: exec_env,
            programs: arg_match.free.clone(),
            clir: arg_match.opt_present("v"),
        })
    }
}

fn main() -> std::io::Result<()> {
    /* Command line parsing */
    let args: Vec<String> = env::args().collect();
    let prog_name = if args.len() > 0 { 
        args[0].clone() 
    } else { 
        "bfrs_jit".to_string()
    }; 

    let cmdline_opts = parser::parse_cmdline(&prog_name, &args).unwrap_or_else(|err_str| {
        println!("Command line errors");
        println!("Command line parse error : {:#?}", err_str);
        parser::usage(&prog_name); 
        std::process::exit(0);
    }); 

    /* Iterate through each BF file */
    for (itr, arg) in cmdline_opts.programs.iter().enumerate() {
        let buffer = std::fs::read(&arg)?; 

        /* Generate program and compile to bytecode */
        let mut prog = match parser::new_program(&buffer, HEAPSIZE) { 
            Ok(program) => program,
            Err(genbc_err) => { 
                println!("Error compiling {} to byte code : {:#?}", arg, genbc_err); 
                continue; 
            }
        };

        /* Execute using user selected execution engine */
        if let Ok((ret, elapsed)) = match cmdline_opts.exec_engine { 
            ExecutionEngine::Interpreter => prog.interpret(), 
            ExecutionEngine::CraneLift => {
                prog.jit_compile(cmdline_opts.clir).or_else(|x| Err("Jit Compilation Error"));
                prog.jit_exec(cmdline_opts.clir).map_err(|x| "Jit execution error")
            }, 
        } { 
            println!("\n============"); 
            println!("prog[{}][{} <{:?}>] returned {}, elapsed-time = {:?}"
                        , itr, arg, cmdline_opts.exec_engine, ret, elapsed); 
            println!("================="); 

            #[cfg(feature="profile")]
            { 
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
                            execute::BFIsa::Incr(n) => { 
                                if *n >= 128 { 
                                    format!("-{}", n.wrapping_neg())
                                } else { 
                                    format!("+{}", n)
                                }
                            }, 
                            execute::BFIsa::Mv(n) => { 
                                if *n < 0 {
                                    format!("<{}", -n)
                                } else { 
                                    format!(">{}", n) 
                                }
                            },
                            execute::BFIsa::In => ",".to_string(),
                            execute::BFIsa::Out => ".".to_string(),
                            execute::BFIsa::Jmp(_) => "[".to_string(),
                            execute::BFIsa::Ret(_) => "]".to_string(),
                            execute::BFIsa::LoopSetZero => "x".to_string(),
                            execute::BFIsa::LoopMvData(n) => {
                                if *n < 0 { 
                                    format!("+<{}", -n)
                                } else {
                                    format!("+>{}", n)
                                }
                            },
                            execute::BFIsa::LoopMvPtr(n) => {
                                if *n < 0 { 
                                    format!("<<{}", -n)
                                } else {
                                    format!(">>{}", n)
                                }

                            },
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
                        loops[idx].1 += loops[idx-1].1;
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
    }
    Ok(())
}
