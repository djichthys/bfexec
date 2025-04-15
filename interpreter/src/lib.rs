
pub mod interpreter {
    pub fn interpret(bytestream: &str) -> Result<u32,u32> {
        println!("{:?}", bytestream);
        Ok(1)
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
