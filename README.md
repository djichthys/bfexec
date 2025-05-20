# An implementation of BF in Rust
This is a Rust implementation of a simple Turing machine executing 
the esoteric [BF](https://esolangs.org/wiki/Brainfuck) language.

Inspired by a series of blog posts 
on [JIT compilation](https://eli.thegreenplace.net/2017/adventures-in-jit-compilation-part-1-an-interpreter) 
by Eli Bendersky. The interpreter is a Rust implementation and JIT compilation is 
done with [Cranelift](https://cranelift.dev/). 

## Command line options
  - *-e* (Intepreter / CraneLift) 
  - *-v* (with -e Cranelift will show generated CLIR) 
  - Cargo run *--features profile* (Only works with -e Interpreter) 
  This will show the list of opcodes, loops and loop structures executed
