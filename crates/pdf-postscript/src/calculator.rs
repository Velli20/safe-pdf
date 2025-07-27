use std::collections::VecDeque;

use crate::operator::Operator;

pub fn parse_tokens(tokens: &[&str]) -> Vec<Operator> {
    let mut i = 0;
    let mut block_stack: Vec<Vec<Operator>> = vec![vec![]];
    while i < tokens.len() {
        match tokens[i] {
            "add" => block_stack.last_mut().unwrap().push(Operator::Add),
            "sub" => block_stack.last_mut().unwrap().push(Operator::Sub),
            "mul" => block_stack.last_mut().unwrap().push(Operator::Mul),
            "div" => block_stack.last_mut().unwrap().push(Operator::Div),
            "dup" => block_stack.last_mut().unwrap().push(Operator::Dup),
            "exch" => block_stack.last_mut().unwrap().push(Operator::Exch),
            "pop" => block_stack.last_mut().unwrap().push(Operator::Pop),
            "eq" => block_stack.last_mut().unwrap().push(Operator::Eq),
            "ne" => block_stack.last_mut().unwrap().push(Operator::Ne),
            "gt" => block_stack.last_mut().unwrap().push(Operator::Gt),
            "lt" => block_stack.last_mut().unwrap().push(Operator::Lt),
            "ge" => block_stack.last_mut().unwrap().push(Operator::Ge),
            "le" => block_stack.last_mut().unwrap().push(Operator::Le),
            "and" => block_stack.last_mut().unwrap().push(Operator::And),
            "or" => block_stack.last_mut().unwrap().push(Operator::Or),
            "not" => block_stack.last_mut().unwrap().push(Operator::Not),
            "if" => {
                let block1 = block_stack.pop().unwrap();
                block_stack.last_mut().unwrap().push(Operator::If(block1));
            }
            "ifelse" => {
                let block1 = block_stack.pop().unwrap();
                let block2 = block_stack.pop().unwrap();
                block_stack
                    .last_mut()
                    .unwrap()
                    .push(Operator::IfElse(block2, block1));
            }
            "{" => {
                block_stack.push(vec![]);
            }
            "}" => {}
            "copy" => block_stack.last_mut().unwrap().push(Operator::Copy),
            "roll" => block_stack.last_mut().unwrap().push(Operator::Roll),
            "sqrt" => block_stack.last_mut().unwrap().push(Operator::Sqrt),
            "cvi" => block_stack.last_mut().unwrap().push(Operator::Cvi),
            "mod" => block_stack.last_mut().unwrap().push(Operator::Mod),

            "truncate" => block_stack.last_mut().unwrap().push(Operator::Truncate),
            "abs" => block_stack.last_mut().unwrap().push(Operator::Abs),

            "true" => block_stack.last_mut().unwrap().push(Operator::Number(1.0)),
            "false" => block_stack.last_mut().unwrap().push(Operator::Number(0.0)),
            t => block_stack
                .last_mut()
                .unwrap()
                .push(Operator::Number(t.parse::<f64>().expect("Invalid number"))),
        }
        i += 1;
    }

    block_stack.pop().unwrap()
}

pub fn execute(input_stack: &[f64], ops: &[Operator]) -> Vec<f64> {
    let mut stack: VecDeque<f64> = input_stack.iter().cloned().collect();

    for op in ops {
        match op {
            Operator::Add => {
                let b = stack.pop_back().unwrap();
                let a = stack.pop_back().unwrap();
                stack.push_back(a + b);
            }
            Operator::Sub => {
                let b = stack.pop_back().unwrap();
                let a = stack.pop_back().unwrap();
                stack.push_back(a - b);
            }
            Operator::Mul => {
                let b = stack.pop_back().unwrap();
                let a = stack.pop_back().unwrap();
                stack.push_back(a * b);
            }
            Operator::Div => {
                let b = stack.pop_back().unwrap();
                let a = stack.pop_back().unwrap();
                stack.push_back(a / b);
            }
            Operator::Dup => {
                let a = *stack.back().unwrap();
                stack.push_back(a);
            }
            Operator::Exch => {
                let b = stack.pop_back().unwrap();
                let a = stack.pop_back().unwrap();
                stack.push_back(b);
                stack.push_back(a);
            }
            Operator::Pop => {
                stack.pop_back();
            }
            Operator::Eq => {
                let b = stack.pop_back().unwrap();
                let a = stack.pop_back().unwrap();
                stack.push_back(if a == b { 1.0 } else { 0.0 });
            }
            Operator::Ne => {
                let b = stack.pop_back().unwrap();
                let a = stack.pop_back().unwrap();
                stack.push_back(if a != b { 1.0 } else { 0.0 });
            }
            Operator::Gt => {
                let b = stack.pop_back().unwrap();
                let a = stack.pop_back().unwrap();
                stack.push_back(if a > b { 1.0 } else { 0.0 });
            }
            Operator::Lt => {
                let b = stack.pop_back().unwrap();
                let a = stack.pop_back().unwrap();
                stack.push_back(if a < b { 1.0 } else { 0.0 });
            }
            Operator::Ge => {
                let b = stack.pop_back().unwrap();
                let a = stack.pop_back().unwrap();
                stack.push_back(if a >= b { 1.0 } else { 0.0 });
            }
            Operator::Le => {
                let b = stack.pop_back().unwrap();
                let a = stack.pop_back().unwrap();
                stack.push_back(if a <= b { 1.0 } else { 0.0 });
            }
            Operator::And => {
                let b = stack.pop_back().unwrap();
                let a = stack.pop_back().unwrap();
                stack.push_back(if a != 0.0 && b != 0.0 { 1.0 } else { 0.0 });
            }
            Operator::Or => {
                let b = stack.pop_back().unwrap();
                let a = stack.pop_back().unwrap();
                stack.push_back(if a != 0.0 || b != 0.0 { 1.0 } else { 0.0 });
            }
            Operator::Not => {
                let a = stack.pop_back().unwrap();
                stack.push_back(if a == 0.0 { 1.0 } else { 0.0 });
            }
            Operator::If(block) => {
                let cond = stack.pop_back().unwrap();
                if cond != 0.0 {
                    // Pass the current stack to the block
                    let mut inner_stack: VecDeque<f64> = stack.clone();
                    let result = execute(&inner_stack.make_contiguous(), &block);
                    stack.clear();
                    for v in result {
                        stack.push_back(v);
                    }
                }
            }
            Operator::IfElse(block1, block2) => {
                let cond = stack.pop_back().unwrap();
                let mut inner_stack: VecDeque<f64> = stack.clone();
                let block = if cond != 0.0 { &block1 } else { &block2 };
                let result = execute(&inner_stack.make_contiguous(), block);
                stack.clear();
                for v in result {
                    stack.push_back(v);
                }
            }
            Operator::Copy => {
                let n = stack.pop_back().unwrap();
                let n = n as usize;
                assert!(n <= stack.len(), "Not enough elements for copy");
                let len = stack.len();
                let mut to_copy = Vec::with_capacity(n);
                for i in 0..n {
                    to_copy.push(stack[len - n + i]);
                }
                for v in &to_copy {
                    stack.push_back(*v);
                }
            }
            Operator::Sqrt => {
                let a = stack.pop_back().unwrap();
                stack.push_back(a.sqrt());
            }
            Operator::Mod => {
                let b = stack.pop_back().unwrap();
                let a = stack.pop_back().unwrap();
                stack.push_back(a % b);
            }
            Operator::Cvi => {
                let a = stack.pop_back().unwrap();
                stack.push_back(a.trunc());
            }
            Operator::Abs => {
                let a = stack.pop_back().unwrap();
                stack.push_back(a.abs());
            }
            Operator::Roll => {
                // PostScript: n m roll
                // Pops m and n, rolls the top n elements by m positions
                let m = stack.pop_back().unwrap() as isize;
                let n = stack.pop_back().unwrap() as usize;
                assert!(n <= stack.len(), "Not enough elements for roll");
                if n == 0 {
                    continue;
                }
                let len = stack.len();
                // Extract the top n elements
                let mut slice: Vec<f64> = stack.drain(len - n..).collect();
                // Normalize m to [0, n)
                let m = ((m % n as isize) + n as isize) % n as isize;
                slice.rotate_right(m as usize);
                // Push back the rotated elements
                for v in slice {
                    stack.push_back(v);
                }
            }
            Operator::Truncate => {
                let a = stack.pop_back().unwrap();
                stack.push_back(a.trunc());
            }
            Operator::Number(num) => stack.push_back(*num),
        }
    }

    stack.into()
}

pub fn evaluate_postscript(input_stack: &[f64], code: &str) -> Vec<f64> {
    // Add whitespace before and after '{' and '}'
    let code = code.replace("{", " { ").replace("}", " } ");
    let ops = parse_tokens(&code.split_whitespace().collect::<Vec<_>>());

    execute(input_stack, &ops)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_operators() {
        let tokens = vec!["add", "sub", "mul", "div"];
        let ops = parse_tokens(&tokens);
        assert_eq!(
            ops,
            vec![Operator::Add, Operator::Sub, Operator::Mul, Operator::Div]
        );
    }

    #[test]
    fn test_parse_numbers() {
        let tokens = vec!["1", "2.5", "-3"];
        let ops = parse_tokens(&tokens);
        assert_eq!(
            ops,
            vec![
                Operator::Number(1.0),
                Operator::Number(2.5),
                Operator::Number(-3.0)
            ]
        );
    }

    #[test]
    fn test_parse_if_block() {
        let tokens = vec!["{", "2", "3", "add", "}", "if"];
        let ops = parse_tokens(&tokens);
        assert_eq!(
            ops,
            vec![Operator::If(vec![
                Operator::Number(2.0),
                Operator::Number(3.0),
                Operator::Add
            ])]
        );
    }

    #[test]
    fn test_parse_ifelse_block() {
        let tokens = vec![
            "{", "2", "3", "add", "}", "{", "4", "5", "add", "}", "ifelse",
        ];
        let ops = parse_tokens(&tokens);
        assert_eq!(
            ops,
            vec![Operator::IfElse(
                vec![Operator::Number(2.0), Operator::Number(3.0), Operator::Add],
                vec![Operator::Number(4.0), Operator::Number(5.0), Operator::Add]
            )]
        );
    }

    #[test]
    fn test_parse_nested_blocks() {
        let tokens = vec!["{", "1", "{", "2", "3", "add", "}", "if", "}", "if"];
        let ops = parse_tokens(&tokens);
        assert_eq!(
            ops,
            vec![Operator::If(vec![
                Operator::Number(1.0),
                Operator::If(vec![
                    Operator::Number(2.0),
                    Operator::Number(3.0),
                    Operator::Add
                ])
            ])]
        );
    }

    #[test]
    #[should_panic(expected = "Invalid number")]
    fn test_parse_invalid_number() {
        let tokens = vec!["foo"];
        let _ = parse_tokens(&tokens);
    }

    #[test]
    fn test_parse_logical_operators() {
        let tokens = vec!["eq", "ne", "gt", "lt", "ge", "le", "and", "or", "not"];
        let ops = parse_tokens(&tokens);
        assert_eq!(
            ops,
            vec![
                Operator::Eq,
                Operator::Ne,
                Operator::Gt,
                Operator::Lt,
                Operator::Ge,
                Operator::Le,
                Operator::And,
                Operator::Or,
                Operator::Not
            ]
        );
    }

    #[test]
    fn test_add() {
        let result = evaluate_postscript(&[2.0, 3.0], "add");
        assert_eq!(result, vec![5.0]);
    }

    #[test]
    fn test_sub() {
        let result = evaluate_postscript(&[5.0, 2.0], "sub");
        assert_eq!(result, vec![3.0]);
    }

    #[test]
    fn test_mul() {
        let result = evaluate_postscript(&[4.0, 3.0], "mul");
        assert_eq!(result, vec![12.0]);
    }

    #[test]
    fn test_div() {
        let result = evaluate_postscript(&[8.0, 2.0], "div");
        assert_eq!(result, vec![4.0]);
    }

    #[test]
    fn test_dup() {
        let result = evaluate_postscript(&[7.0], "dup");
        assert_eq!(result, vec![7.0, 7.0]);
    }

    #[test]
    fn test_exch() {
        let result = evaluate_postscript(&[1.0, 2.0], "exch");
        assert_eq!(result, vec![2.0, 1.0]);
    }

    #[test]
    fn test_pop() {
        let result = evaluate_postscript(&[1.0, 2.0, 3.0], "pop");
        assert_eq!(result, vec![1.0, 2.0]);
    }

    #[test]
    fn test_eq() {
        let result = evaluate_postscript(&[2.0, 2.0], "eq");
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[2.0, 3.0], "eq");
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_ne() {
        let result = evaluate_postscript(&[2.0, 3.0], "ne");
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[2.0, 2.0], "ne");
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_gt() {
        let result = evaluate_postscript(&[3.0, 2.0], "gt");
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[2.0, 3.0], "gt");
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_lt() {
        let result = evaluate_postscript(&[2.0, 3.0], "lt");
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[3.0, 2.0], "lt");
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_ge() {
        let result = evaluate_postscript(&[3.0, 2.0], "ge");
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[2.0, 2.0], "ge");
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[1.0, 2.0], "ge");
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_le() {
        let result = evaluate_postscript(&[2.0, 3.0], "le");
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[2.0, 2.0], "le");
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[3.0, 2.0], "le");
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_and() {
        let result = evaluate_postscript(&[1.0, 1.0], "and");
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[1.0, 0.0], "and");
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_or() {
        let result = evaluate_postscript(&[0.0, 1.0], "or");
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[0.0, 0.0], "or");
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_not() {
        let result = evaluate_postscript(&[0.0], "not");
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[1.0], "not");
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_if_true() {
        let result = evaluate_postscript(&[1.0], "{ 2 3 add } if");
        assert_eq!(result, vec![5.0]);
    }

    #[test]
    fn test_if_false() {
        let result = evaluate_postscript(&[0.0], "{ 2 3 add } if");
        assert_eq!(result, Vec::<f64>::new());
    }

    #[test]
    fn test_ifelse_true() {
        let result = evaluate_postscript(&[1.0], "{ 2 3 add } { 4 5 add } ifelse");
        assert_eq!(result, vec![5.0]);
    }

    #[test]
    fn test_ifelse_false() {
        let result = evaluate_postscript(&[0.0], "{ 2 3 add } { 4 5 add } ifelse");
        assert_eq!(result, vec![9.0]);
    }

    #[test]
    fn test_nested_blocks() {
        let result = evaluate_postscript(&[1.0], "{ 1 { 2 3 add } if } if");
        assert_eq!(result, vec![5.0]);
    }

    #[test]
    fn test_complex_expression() {
        let result = evaluate_postscript(&[2.0, 3.0, 4.0], "add mul");
        // (3 + 4) = 7, 2 * 7 = 14
        assert_eq!(result, vec![14.0]);
    }

    #[test]
    fn test_copy() {
        let result = evaluate_postscript(&[1.0, 2.0, 3.0], "2 copy");
        // Stack: [1.0, 2.0, 3.0] -> pop 2, copy top 2: [2.0, 3.0], push: [1.0, 2.0, 3.0, 2.0, 3.0]
        assert_eq!(result, vec![1.0, 2.0, 3.0, 2.0, 3.0]);
    }

    #[test]
    fn test_roll() {
        // [1 2 3 4 5] 3 1 roll -> [1 2 5 3 4]
        let result = evaluate_postscript(&[1.0, 2.0, 3.0, 4.0, 5.0], "3 1 roll");
        assert_eq!(result, vec![1.0, 2.0, 5.0, 3.0, 4.0]);
        // [1 2 3 4 5] 4 -2 roll -> [1 4 5 2 3]
        let result = evaluate_postscript(&[1.0, 2.0, 3.0, 4.0, 5.0], "4 -2 roll");
        assert_eq!(result, vec![1.0, 4.0, 5.0, 2.0, 3.0]);
        // [1 2 3 4 5] 0 7 roll -> [1 2 3 4 5] (no change)
        let result = evaluate_postscript(&[1.0, 2.0, 3.0, 4.0, 5.0], "0 7 roll");
        assert_eq!(result, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn test_sqrt() {
        let result = evaluate_postscript(&[9.0], "sqrt");
        assert_eq!(result, vec![3.0]);
        let result = evaluate_postscript(&[2.25], "sqrt");
        assert_eq!(result, vec![1.5]);
    }

    #[test]
    fn test_truncate() {
        let result = evaluate_postscript(&[3.7], "truncate");
        assert_eq!(result, vec![3.0]);
        let result = evaluate_postscript(&[-2.9], "truncate");
        assert_eq!(result, vec![-2.0]);
        let result = evaluate_postscript(&[0.0], "truncate");
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_abs() {
        let result = evaluate_postscript(&[-5.0], "abs");
        assert_eq!(result, vec![5.0]);
        let result = evaluate_postscript(&[3.2], "abs");
        assert_eq!(result, vec![3.2]);
        let result = evaluate_postscript(&[0.0], "abs");
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_cvi() {
        let result = evaluate_postscript(&[3.7], "cvi");
        assert_eq!(result, vec![3.0]);
        let result = evaluate_postscript(&[-2.9], "cvi");
        assert_eq!(result, vec![-2.0]);
        let result = evaluate_postscript(&[0.0], "cvi");
        assert_eq!(result, vec![0.0]);
        let result = evaluate_postscript(&[5.0], "cvi");
        assert_eq!(result, vec![5.0]);
    }

    #[test]
    fn test_mod() {
        let result = evaluate_postscript(&[10.0, 3.0], "mod");
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[-10.0, 3.0], "mod");
        assert_eq!(result, vec![-1.0]);
        let result = evaluate_postscript(&[10.0, -3.0], "mod");
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[0.0, 3.0], "mod");
        assert_eq!(result, vec![0.0]);
    }
}
