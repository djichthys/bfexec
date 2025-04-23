#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum BFIsa { 
    Incr(u8),
    Out,
    In,
    Mv(isize),
    LoopSetZero,
    LoopMvData(isize),
    LoopMvPtr(isize),
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
    pub loopsetz: u64,
    pub loopmvptr: u64,
    pub loopmvdata: u64,
    pub loops: std::collections::HashMap<std::ops::Range<usize>, usize>,
}

pub struct NestingErr(&'static str, usize);

pub struct ProgramState { 
    ptr:  usize,
    pc:   usize,
    heap: Vec<u8>, 
    pub txt:  Vec<BFIsa>,
    #[cfg(feature = "profile")]
    pub profile: Profile,
}



impl ProgramState { 
    pub fn new(src: &[u8], heap_sz: usize) -> Result<ProgramState, NestingErr> { 
        let mut code = Vec::new();
        let mut nest_stk = Vec::new();

        for (pos, byte) in src.iter().enumerate() {
            let instr = match byte { 
                b'+' | b'-' => { 
                    let incr = if *byte == b'+' {1} else {1u8.wrapping_neg()}; 
                    if let Some(BFIsa::Incr(rhs)) = code.last_mut() { 
                        *rhs = rhs.wrapping_add(incr);
                        continue;
                    }
                    BFIsa::Incr(incr)
                },
                b'.' => BFIsa::Out,
                b',' => BFIsa::In,
                b'>' | b'<' => {
                    let incr = if *byte == b'>' {1} else {-1}; 
                    if let Some(BFIsa::Mv(curr)) = code.last_mut() {
                        *curr += incr; 
                        continue;
                    }; 
                    BFIsa::Mv(incr) 
                }, 
                b'[' => { 
                    nest_stk.push((code.len(), pos));
                    BFIsa::Jmp(0)
                },
                b']' => { 
                    if let Some((ret_addr, _loc)) = nest_stk.pop() { 
                        code[ret_addr] = BFIsa::Jmp(code.len());

                        match code.as_slice() { 
                            [.., BFIsa::Jmp(_), BFIsa::Incr(n)] if n % 2 == 1 => {
                                code.drain(code.len() - 2..); 
                                BFIsa::LoopSetZero
                            }, 

                            &[.., BFIsa::Jmp(_), BFIsa::Incr(255), BFIsa::Mv(pdat), BFIsa::Incr(1), BFIsa::Mv(pidx)]
                                if pdat == pidx => {
                                    code.drain(code.len() - 5..);
                                    BFIsa::LoopMvData(pdat)
                            },

                            &[.., BFIsa::Jmp(_), BFIsa::Mv(pptr)] => {
                                code.drain(code.len() - 2..);
                                BFIsa::LoopMvPtr(pptr)
                            },

                            _ => BFIsa::Ret(ret_addr),
                        }
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

        Ok(ProgramState { 
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
                    BFIsa::Incr(_) => self.profile.arith += 1,
                    BFIsa::Out => self.profile.out += 1,
                    BFIsa::In => self.profile.inp += 1,
                    BFIsa::Mv(_) => self.profile.mv += 1,
                    BFIsa::Jmp(_) => self.profile.jmp += 1,
                    BFIsa::Ret(addr) => {
                        self.profile.ret += 1;
                        *self.profile
                            .loops
                            .entry(addr..self.pc+1).
                            or_default() += 1;
                    },
                    BFIsa::LoopSetZero => self.profile.loopsetz += 1,
                    BFIsa::LoopMvData(_) => self.profile.loopmvdata += 1,
                    BFIsa::LoopMvPtr(_) => self.profile.loopmvptr += 1,
                }
                    
            }



            match self.txt[self.pc] { 
                BFIsa::Incr(rhs) => self.heap[self.ptr] = self.heap[self.ptr].wrapping_add(rhs), 
                BFIsa::Out => print!("{}", self.heap[self.ptr] as char),
                BFIsa::In => { 
                    use std::io::Read; 
                    let _ = match std::io::stdin().read_exact(&mut self.heap[self.ptr..self.ptr+1]) { 
                        Ok(()) => 0, 
                        Err(_) => { return Err("Error reading from stdio"); }, 
                    };
                }, 
                BFIsa::Mv(disp) => { 
                    let heap_sz = self.heap.len() as isize; 
                    let disp = (heap_sz + (disp % heap_sz)) as usize; 
                    self.ptr = (self.ptr + disp) % heap_sz as usize; 
                }, 
                BFIsa::LoopSetZero => { 
                    self.heap[self.ptr] = 0;
                },
                BFIsa::LoopMvData(n) => { 
                    let len = self.heap.len() as isize; 
                    let n = (len + n % len) as usize; 
                    let to = (self.ptr + n) % len as usize;

                    self.heap[to] = self.heap[to].wrapping_add(self.heap[self.ptr]);
                    self.heap[self.ptr] = 0;
                },
                BFIsa::LoopMvPtr(n) => { 
                    let len = self.heap.len() as isize; 
                    let n = (len + n % len) as usize; 
                    loop { 
                        if self.heap[self.ptr] == 0 {
                            break;
                        }
                        self.ptr = (self.ptr + n) % len as usize;
                    }

                },
                BFIsa::Jmp(target) => { 
                    if self.heap[self.ptr] == 0 { 
                        self.pc = target; 
                    }
                },
                BFIsa::Ret(target) => { 
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
