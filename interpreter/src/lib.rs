#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum BF_ISA { 
    Incr(u8),
    Out,
    In,
    Mv(isize),
    Jmp(usize), 
    Ret(usize),
}

#[derive(Default,Debug)]
#[cfg(feature = "profile")]
pub struct Profile { 
    pub arith: u64,
    pub mv:  u64,
    pub inp: u64, 
    pub out: u64, 
    pub jmp: u64,
    pub ret: u64,
    pub loops: std::collections::HashMap<std::ops::Range<usize>, usize>,
}

pub struct NestingErr(&'static str, usize);

pub struct Program_State { 
    ptr:  usize,
    pc:   usize,
    heap: Vec<u8>, 
    pub txt:  Vec<BF_ISA>,
    #[cfg(feature = "profile")]
    pub profile: Profile,
}



impl Program_State { 
    pub fn new(src: &[u8], heap_sz: usize) -> Result<Program_State, NestingErr> { 
        let mut code = Vec::new();
        let mut nest_stk = Vec::new();

        for (pos, byte) in src.iter().enumerate() {
            let instr = match byte { 
                b'+' | b'-' => { 
                    let incr = if *byte == b'+' {1} else {1u8.wrapping_neg()}; 
                    if let Some(BF_ISA::Incr(rhs)) = code.last_mut() { 
                        *rhs = rhs.wrapping_add(incr);
                        continue;
                    }
                    BF_ISA::Incr(incr)
                },
                b'.' => BF_ISA::Out,
                b',' => BF_ISA::In,
                b'>' | b'<' => {
                    let incr = if *byte == b'>' {1} else {-1}; 
                    if let Some(BF_ISA::Mv(curr)) = code.last_mut() {
                        *curr += incr; 
                        continue;
                    }; 
                    BF_ISA::Mv(incr) 
                }, 
                b'[' => { 
                    nest_stk.push((code.len(), pos));
                    BF_ISA::Jmp(0)
                },
                b']' => { 
                    if let Some((ret_addr, _loc)) = nest_stk.pop() { 
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

        if let Some((_unpaired_jmp, pos)) = nest_stk.pop() { 
            return Err(NestingErr("Nesting Err [ @", pos));
        }

        Ok(Program_State { 
            ptr:  0, 
            pc:   0, 
            heap: vec![0; heap_sz], 
            txt:  code,
            #[cfg(feature = "profile")] 
            profile: Profile::default()
        })
    }

    pub fn interpret(&mut self) -> Result<i32, &'static str> {
        'program: loop { 
            #[cfg(feature = "profile")]
            {
                match self.txt[self.pc] { 
                    BF_ISA::Incr(_) => self.profile.arith += 1,
                    BF_ISA::Out => self.profile.out += 1,
                    BF_ISA::In => self.profile.inp += 1,
                    BF_ISA::Mv(_) => self.profile.mv += 1,
                    BF_ISA::Jmp(_) => self.profile.jmp += 1,
                    BF_ISA::Ret(addr) => {
                        self.profile.ret += 1;
                        *self.profile
                            .loops
                            .entry(addr..self.pc+1).
                            or_default() += 1;
                    },
                }
                    
            }



            match self.txt[self.pc] { 
                BF_ISA::Incr(rhs) => self.heap[self.ptr] = self.heap[self.ptr].wrapping_add(rhs), 
                BF_ISA::Out => print!("{}", self.heap[self.ptr] as char),
                BF_ISA::In => { 
                    use std::io::Read; 
                    let _ = match std::io::stdin().read_exact(&mut self.heap[self.ptr..self.ptr+1]) { 
                        Ok(()) => 0, 
                        Err(_) => { return Err("Error reading from stdio"); }, 
                    };
                }, 
                BF_ISA::Mv(disp) => { 
                    let heap_sz = self.heap.len() as isize; 
                    let disp = (heap_sz + (disp % heap_sz)) as usize; 
                    self.ptr = (self.ptr + disp) % heap_sz as usize; 
                }, 
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
