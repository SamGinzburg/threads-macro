use z3::{Config, Context, Solver};
use std::collections::HashMap;

pub fn solve_constraints(ctx: z3::Context, lst: Vec<Vec<String>>) -> z3::SatResult {
    // initialize all the variables
    // do a first pass of the lst, for each unique var, create a solver const
    // each possible lock ordering is represented by a list
    // we need the dict for the first pass, and the second pass
    let mut seen_dict = HashMap::new();
    let mut count: u64 = 0;
    for lock_ordering in &lst {
        // we need to add each constant
        for constant in lock_ordering {
            match seen_dict.get(constant) {
                Some(_) => {
                    // if we have seen the constant before
                    // no-op
                },
                None => {
                    // if we haven't seen the constant before
                    // add it to the solver
                    let var_name = format!("var{}", count);
                    let x = z3::ast::Int::new_const(&ctx, var_name);
                    seen_dict.insert(constant, x);
                    count += 1;
                }
            }
        }
    }


    // add in the constraints
    // do the second pass, adding in the constraints
    // we use the dict from the previous iteration to get the solver constants
    let solver = Solver::new(&ctx);
    let mut previous_constant: Option<&z3::ast::Int> = None;
    for lock_ordering in &lst {
        for constant in lock_ordering {
            match previous_constant {
                Some(c1) => {
                    // we need to get the current constant
                    match seen_dict.get(constant) {
                        Some(c2) => {
                            // generate the constraint: c1 < c2
                            solver.assert(&c1.lt(&c2));
                        },
                        None => {
                            // This should never happen, if so, we missed a constant
                            // during the first pass
                            panic!("Unable to find constant: ({}) in dict during second pass", constant);
                        }
                    }
                },
                None => {
                    // if we have no previous const, this is the first iteration
                    // of the loop
                    // No constraints are added yet, we just need to set the previous_constant
                    match seen_dict.get(constant) {
                        Some(x) => {
                            previous_constant = Some(x);
                        },
                        None => {
                            // this should not happen ever! It means we missed a variable during our first pass
                            panic!("Unable to find constant: ({}) in dict during second pass", constant);
                        }
                    }
                }
            }
        }
        // after each loop reset the previous_constant to None
        previous_constant = None;
    }
    solver.check()
}