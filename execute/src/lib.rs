use cranelift::{
    codegen::{
        entity::EntityRef,
        ir::{condcodes::IntCC, types::I8, AbiParam, function::Function, InstBuilder, MemFlags, Signature, UserFuncName},
        isa,
        settings::{self, Configurable},
        verify_function,
        control,
        Context,
    },
    frontend::{FunctionBuilder, FunctionBuilderContext, Variable},
};

use target_lexicon::Triple;
use memmap2;
use std::io::{Read,Write};



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

#[derive(Debug)]
pub struct NestingErr(&'static str, usize);

#[derive(Debug)]
pub struct JitErr(&'static str);

pub struct ProgramState { 
    ptr:  usize,
    pc:   usize,
    heap: Vec<u8>, 
    pub txt:  Vec<BFIsa>,
    jit_txt: Option<Vec<u8>>,
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
                                if pdat == -pidx => {
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
            ptr: 0, 
            pc: 0, 
            heap: vec![0; heap_sz], 
            txt: code, 
            jit_txt: None,
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

    pub fn jit_compile(&mut self, clir: bool) -> Result<i32, JitErr> {
        // Compiler setup  
        let mut builder = settings::builder();
        builder.set("opt_level", "speed").unwrap(); 
        builder.set("preserve_frame_pointers", "false").unwrap(); 

        let flags = settings::Flags::new(builder); 
        let isa = match isa::lookup(Triple::host()) { 
            Err(err) => panic!("Error looking up target : {}", err), 
            Ok(isa_builder) => isa_builder.finish(flags).unwrap(), 
        };

        // Set up runtime interface
        let call_conv = isa::CallConv::triple_default(isa.triple());
        let pointer_type = isa.pointer_type(); 
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(pointer_type));
        sig.returns.push(AbiParam::new(pointer_type));

        let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig);
        let mut func_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut func_ctx);

        let ptr = Variable::new(0);
        builder.declare_var(ptr, pointer_type);

        let exit_block = builder.create_block();
        builder.append_block_param(exit_block, pointer_type); 

        let block = builder.create_block();
        builder.seal_block(block);
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);

        let heap = builder.block_params(block)[0];  // First param to block - heap pointer
        let zero_byte = builder.ins().iconst(I8, 0);
        let zero = builder.ins().iconst(pointer_type, 0);
        builder.def_var(ptr, zero);

        let mem_flags = MemFlags::new();

        let (write_sig, write_address) = { 
            let mut write_sig = Signature::new(call_conv); 
            write_sig.params.push(AbiParam::new(I8));
            write_sig.returns.push(AbiParam::new(pointer_type));
            let write_sig = builder.import_signature(write_sig); 

            let write_address = write as *const () as i64;
            let write_address = builder.ins().iconst(pointer_type, write_address); 
            (write_sig, write_address)
        };

        let (read_sig, read_address) = { 
            let mut read_sig = Signature::new(call_conv); 
            read_sig.params.push(AbiParam::new(pointer_type));
            read_sig.returns.push(AbiParam::new(pointer_type));
            let read_sig = builder.import_signature(read_sig); 

            let read_address = read as *const () as i64;
            let read_address = builder.ins().iconst(pointer_type, read_address); 
            (read_sig, read_address)
        };

        /* stack to hold nested '[' operators */

        let mut nest_stk = Vec::new();

        for (idx, instr) in self.txt.iter().enumerate() { 
            match instr { 
                BFIsa::Incr(n) => { 
                    let n = *n as i64;
                    let heap_offset = builder.use_var(ptr);
                    let heap_ptr = builder.ins().iadd(heap, heap_offset);
                    let val_at_heap_ptr = builder.ins().load(I8, mem_flags, heap_ptr, 0);
                    let val_at_heap_ptr = builder.ins().iadd_imm(val_at_heap_ptr, n);
                    builder.ins().store(mem_flags, val_at_heap_ptr, heap_ptr, 0);
                },
                BFIsa::Mv(n) => { 
                    let n = *n as i64;
                    let heap_offset = builder.use_var(ptr);
                    let tgt_heap_offset = builder.ins().iadd_imm(heap_offset, n);

                    let new_heap_offset = if n > 0  {
                        let wrapped = builder.ins().iadd_imm( heap_offset, n - (self.heap.len() as i64)); 
                        let cmp = builder.ins().icmp_imm(IntCC::SignedLessThan, tgt_heap_offset, self.heap.len() as i64);
                        builder.ins().select(cmp, tgt_heap_offset, wrapped) 
                    } else { 
                        let wrapped = builder.ins().iadd_imm( heap_offset, n + (self.heap.len() as i64)); 
                        let cmp = builder.ins().icmp_imm(IntCC::SignedLessThan, tgt_heap_offset, 0);
                        builder.ins().select(cmp, wrapped, tgt_heap_offset)
                    };

                    builder.def_var(ptr, new_heap_offset);
                },
                BFIsa::Out => { 
                    let heap_offset = builder.use_var(ptr);
                    let heap_ptr = builder.ins().iadd(heap, heap_offset);
                    let fn_arg_val = builder.ins().load(I8, mem_flags, heap_ptr, 0);

                    let call_writefn = builder.ins().call_indirect(write_sig, write_address, &[fn_arg_val]);
                    let call_retval = builder.inst_results(call_writefn)[0];

                    let bb_ret = builder.create_block(); 
                    builder.ins().brif(call_retval, exit_block, &[call_retval], bb_ret, &[]); 

                    builder.seal_block(bb_ret); 
                    builder.switch_to_block(bb_ret); 
                }, 
                BFIsa::In => { 
                    let heap_offset = builder.use_var(ptr);
                    let heap_ptr = builder.ins().iadd(heap, heap_offset);
                    let call_readfn = builder.ins().call_indirect(read_sig, read_address, &[heap_ptr]);
                    let call_retval = builder.inst_results(call_readfn)[0];

                    let bb_ret = builder.create_block(); 
                    builder.ins().brif(call_retval, exit_block, &[call_retval], bb_ret, &[]); 

                    builder.seal_block(bb_ret); 
                    builder.switch_to_block(bb_ret); 
                },
                BFIsa::Jmp(_) => { 
                    let inner_bb = builder.create_block(); 
                    let inner_bb_exit = builder.create_block(); 

                    let heap_offset = builder.use_var(ptr);
                    let heap_ptr = builder.ins().iadd(heap, heap_offset); 
                    let val_at_heap_ptr = builder.ins().load(I8, mem_flags, heap_ptr, 0);  

                    builder.ins().brif(val_at_heap_ptr, inner_bb, &[], inner_bb_exit, &[]); // goto ']' if ptr == 0
                    builder.switch_to_block(inner_bb); 

                    nest_stk.push((inner_bb, inner_bb_exit)); // finish both BBlocks when popping stack
                },
                BFIsa::Ret(_) => {
                    let (curr_bb, exit_bb) = match nest_stk.pop() { 
                        Some((bb_expr, bb_exit)) => (bb_expr, bb_exit), 
                        None => return Err(JitErr("Nesting Err in byte code ]")),
                    };

                    let heap_offset = builder.use_var(ptr);
                    let heap_ptr = builder.ins().iadd(heap, heap_offset); 
                    let val_at_heap_ptr = builder.ins().load(I8, mem_flags, heap_ptr, 0);  

                    builder.ins().brif(val_at_heap_ptr, curr_bb, &[], exit_bb, &[]); // goto '[' if ptr != 0
                    builder.seal_block(curr_bb);
                    builder.seal_block(exit_bb);
                    builder.switch_to_block(exit_bb); 
                },
                BFIsa::LoopSetZero => {
                    let heap_offset = builder.use_var(ptr);
                    let heap_ptr = builder.ins().iadd(heap, heap_offset); 
                    builder.ins().store(mem_flags, zero_byte, heap_ptr, 0);
                }, 
                BFIsa::LoopMvData(n) => {
                    let n = *n as i64; 
                    let heap_offset = builder.use_var(ptr);
                    let tgt_heap_offset = builder.ins().iadd_imm(heap_offset, n); 

                    let tgt_heap_offset = if n > 0 {
                        let wrapped = builder.ins().iadd_imm(heap_offset, n - (self.heap.len() as i64));
                        let cmp = builder.ins().icmp_imm(IntCC::SignedLessThan, tgt_heap_offset, self.heap.len() as i64);
                        builder.ins().select(cmp, tgt_heap_offset, wrapped) 
                    } else { 
                        let wrapped = builder.ins().iadd_imm(heap_offset, n + (self.heap.len()as i64));
                        let cmp = builder.ins().icmp_imm(IntCC::SignedLessThan, tgt_heap_offset, 0);
                        builder.ins().select(cmp, wrapped, tgt_heap_offset)
                    };

                    let rhs_ptr = builder.ins().iadd(heap, heap_offset); 
                    let rhs_val = builder.ins().load(I8, mem_flags, rhs_ptr, 0); 

                    let lhs_ptr = builder.ins().iadd(heap, tgt_heap_offset); 
                    let lhs_val = builder.ins().load(I8, mem_flags, lhs_ptr, 0); 

                    let sum = builder.ins().iadd(lhs_val, rhs_val);
                    builder.ins().store(mem_flags, sum, lhs_ptr, 0);
                    builder.ins().store(mem_flags, zero_byte, rhs_ptr, 0);
                },
                BFIsa::LoopMvPtr(n) => { 
                    let n = *n as i64; 
                    let loop_bb = builder.create_block(); 
                    let loop_bb_exit = builder.create_block(); 

                    /* Load from ptr variable */
                    let heap_offset = builder.use_var(ptr);
                    let heap_ptr = builder.ins().iadd(heap, heap_offset); 
                    let val_at_heap_ptr = builder.ins().load(I8, mem_flags, heap_ptr, 0);  
                    builder.ins().brif(val_at_heap_ptr, loop_bb, &[], loop_bb_exit, &[]); // goto ']' if ptr == 0

                    builder.switch_to_block(loop_bb); 
                    /* Load from heap-ptr variable each time due to current BB updating it */
                    let heap_offset = builder.use_var(ptr);
                    let heap_ptr = builder.ins().iadd(heap, heap_offset); 
                    let tgt_heap_offset = builder.ins().iadd_imm(heap_offset, n); 
                    let tgt_heap_offset = if n > 0 { 
                        let wrapped = builder.ins().iadd_imm(heap_offset, n - (self.heap.len() as i64));
                        let cmp = builder.ins().icmp_imm(IntCC::SignedLessThan, tgt_heap_offset, self.heap.len() as i64);
                        builder.ins().select(cmp, tgt_heap_offset, wrapped)
                    } else { 
                        let wrapped = builder.ins().iadd_imm(heap_offset, n + (self.heap.len()as i64));
                        let cmp = builder.ins().icmp_imm(IntCC::SignedLessThan, tgt_heap_offset, 0);
                        builder.ins().select(cmp, wrapped, tgt_heap_offset)
                    };
                    let loop_data_ptr = builder.ins().iadd(heap, tgt_heap_offset); 
                    builder.def_var(ptr, tgt_heap_offset);
                    let loop_data_val = builder.ins().load(I8, mem_flags, loop_data_ptr, 0); 
                    builder.ins().brif(loop_data_val, loop_bb, &[], loop_bb_exit, &[]); // goto ']' if ptr == 0
                    builder.seal_block(loop_bb);
                    builder.seal_block(loop_bb_exit);

                    builder.switch_to_block(loop_bb_exit); 
                }, 
            }
        }

        // Post processing
        builder.ins().return_(&[zero]);
        builder.switch_to_block(exit_block); 
        builder.seal_block(exit_block); 

        let result = builder.block_params(exit_block)[0];
        builder.ins().return_(&[result]); 
        builder.finalize(); 
        let verified = verify_function(&func, &*isa); 
        if let Err(errors) = verified { 
            panic!("error message = {}", errors); 
        }; 

        let mut ctx = Context::for_function(func); 
        let mut ctrl_plane =control::ControlPlane::default();

        let code = match ctx.compile(&*isa, &mut ctrl_plane) { 
            Ok(cc) => cc,
            Err(err) => { 
                println!("Error Compiling code = {:?}", err); 
                if clir { 
                    println!("Compiled Code: ====================\n{}", ctx.func.display()); 
                } 
                std::process::exit(-1);
            },
        };

        let code = code.buffer.data().to_vec();
        if clir { 
            println!("Compiled Code: ====================\n{}", ctx.func.display()); 
        }

        self.jit_txt = Some(code);  // package code 

        if clir { 
            println!("Compiled code buffer = {:?}", self.jit_txt);
        }

        Ok(0)
    }


    pub fn jit_exec(&mut self, clir: bool) -> Result<i32, JitErr> {
        let code = match &self.jit_txt {
            Some(code_txt) => code_txt,
            None => { 
                return Ok(0); 
            },
        }; 

        let mut buff = memmap2::MmapOptions::new() 
            .len(code.len())
            .map_anon()
            .unwrap();

        buff.copy_from_slice(code);
        let buff = buff.make_exec().unwrap();
        unsafe { 
            let jit_fn : unsafe extern "C" fn(*mut u8) -> *mut usize = 
                std::mem::transmute(buff.as_ptr());
            let error = jit_fn(self.heap.as_mut_ptr());
            return Ok(error as i32);
        }
        Ok(0)
    }

}

extern "C" fn write(value: u8) -> *mut std::io::Error { 
    let mut stdout = std::io::stdout().lock(); 
    let result = stdout.write_all(&[value]).and_then(|_| stdout.flush()); 

    match result { 
        Err(err) => Box::into_raw(Box::new(err)),
        _ => std::ptr::null_mut(),
    }
}


unsafe extern "C" fn read(buf: *mut u8) -> *mut std::io::Error { 
    let mut stdin = std::io::stdin().lock(); 
    let mut value = 0; 
    let err = stdin.read_exact(std::slice::from_mut(&mut value));
    if let Err(err) = err { 
        if err.kind() != std::io::ErrorKind::UnexpectedEof { 
            return Box::into_raw(Box::new(err)); 
        } 
        value = 0; 
    }

    *buf = value;
    std::ptr::null_mut()
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
