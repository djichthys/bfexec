#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum BF_ISA { 
    Incr,
    Decr,
    Out,
    In,
    MvRight,
    MvLeft,
    Jmp(usize), 
    Ret(usize),
}

pub struct NestingErr(&'static str, usize);

pub struct Program_State { 
    ptr:  usize,
    pc:   usize,
    heap: Vec<u8>, 
    txt:  Vec<BF_ISA>
}


impl Program_State { 
    pub fn new(src: &[u8], heap_sz: usize) -> Result<Program_State, NestingErr> { 
        let mut code = Vec::new();
        let mut nest_stk = Vec::new();

        for (pos, byte) in src.iter().enumerate() {
            let instr = match byte { 
                b'+' => BF_ISA::Incr,
                b'-' => BF_ISA::Decr,
                b'.' => BF_ISA::Out,
                b',' => BF_ISA::In,
                b'>' => BF_ISA::MvRight,
                b'<' => BF_ISA::MvLeft,
                b'[' => { 
                    nest_stk.push((code.len(), pos));
                    BF_ISA::Jmp(0)
                },
                b']' => { 
                    if let Some((ret_addr, loc)) = nest_stk.pop() { 
                        code[ret_addr] = BF_ISA::Jmp(code.len());
                        BF_ISA::Ret(ret_addr)
                    } else {
                        return Err(NestingErr("Nesting Err ] @", pos));
                    }
                },
                _ => {
                    continue; 
                }
            };
            code.push(instr);
        }

        if let Some((unpaired_jmp, pos)) = nest_stk.pop() { 
            return Err(NestingErr("Nesting Err [ @", pos));
        }

        /*
        let code: Vec<BF_ISA> = src.into_iter().filter_map( |byte| { 
            match byte { 
                b'+' => Some(BF_ISA::Incr),
                b'-' => Some(BF_ISA::Decr),
                b'.' => Some(BF_ISA::Out),
                b',' => Some(BF_ISA::In),
                b'>' => Some(BF_ISA::MvRight),
                b'<' => Some(BF_ISA::MvLeft),
                b'[' => Some(BF_ISA::Jmp),
                b']' => Some(BF_ISA::Ret),
                _    => None 
            }
        }).collect(); 
        */

        Ok(Program_State { 
            ptr:  0, 
            pc:   0, 
            heap: vec![0; heap_sz], 
            txt:  code 
        })
    }

    pub fn interpret(&mut self) -> Result<i32, &'static str> {
        'program: loop { 
            match self.txt[self.pc] { 
                BF_ISA::Incr => self.heap[self.ptr] = self.heap[self.ptr].wrapping_add(1),
                BF_ISA::Decr => self.heap[self.ptr] = self.heap[self.ptr].wrapping_sub(1),
                BF_ISA::Out => print!("{}", self.heap[self.ptr] as char),
                BF_ISA::In => { 
                    use std::io::Read; 
                    let _ = match std::io::stdin().read_exact(&mut self.heap[self.ptr..self.ptr+1]) { 
                        Ok(()) => 0, 
                        Err(_) => { return Err("Error reading from stdio"); }, 
                    };
                }, 
                BF_ISA::MvRight => self.ptr = (self.ptr+1) % self.heap.len(), 
                BF_ISA::MvLeft => self.ptr = (self.ptr + self.heap.len() - 1) % self.heap.len(),
                BF_ISA::Jmp(target) => { 
                    if self.heap[self.ptr] == 0 { 
                        self.pc = target; 
                    }
                },
                BF_ISA::Ret(target) => { 
                    if self.heap[self.ptr] != 0 { 
                        self.pc = target; 
                    }
                }
            }

            self.pc += 1;

            if self.txt.len() == self.pc {
                break 'program;
            }
        }
        Ok(0)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
