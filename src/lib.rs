#![feature(box_patterns)]
#![feature(duration_float)]

extern crate proc_macro;
extern crate syn;
extern crate regex;
extern crate z3;
extern crate quote;

mod solver;

use regex::Regex;
use proc_macro::TokenStream;
use syn::Expr;
use syn::Stmt::*;
use syn::Expr::*;
use std::collections::HashMap;
use z3::{Config, Context};
use solver::solve_constraints;
use quote::{quote, format_ident};
use syn::export::TokenStream2;

fn extend_orderings(first: Vec<Vec<String>>, second: Vec<Vec<String>>) -> Vec<Vec<String>> {
    let mut final_lst: Vec<Vec<String>> = vec![];

    if first.len() > 0 && second.len() == 0 {
        final_lst = first;
    } else if first.len() == 0 && second.len() > 0 {
        final_lst = second;
    } else if first.len() > 0 && second.len() > 0 {
        for string_one in &first {
            for string_two in &second {
                let mut temp1: Vec<String> = string_one.clone();
                temp1.extend(string_two.clone());
                final_lst.push(temp1);
            }
        }

    }

    final_lst
}

fn parse_stmts_vec(stmts: Vec<syn::Stmt>,
                   lock_subst: HashMap<String, String>,
                   in_lst: Vec<Vec<String>>) -> Vec<Vec<String>> {
    let mut final_orderings: Vec<Vec<String>> = in_lst.clone();
    for statement in stmts {
        println!("{:?}", statement);
        let orderings = match statement {
            Local(l) => {
                //let pat = l.clone().pat;
                let init = l.clone().init;
                match init {
                    Some((_, expr)) => {
                        // TODO: when binding lets, we need to keep track of them
                        // We must disallow direct calls
                        parse_ast(*expr, lock_subst.clone(), in_lst.clone())
                    },
                    None => {
                        vec![]
                    },
                }
            },
            Item(i) => {
                vec![]
            },
            Expr(nested_e) => {
                parse_ast(nested_e, lock_subst.clone(), in_lst.clone())
            },
            Semi(nested_e, _) => {
                parse_ast(nested_e, lock_subst.clone(), in_lst.clone())
            },
        };
        /*
         * At the end of each iteration we have a new set of orderings for each statement
         * since each statement can be another block of expressions.
         * We need to properly extend the previous set of orderings multiplicatively.
         * EX: if we previously had [["a", "b"]], and we just obtained [["b"], ["c"]],
         * we get a new set of orderings: [["a", "b", "b"], ["a", "b", "c"]]
         */
        final_orderings = extend_orderings(final_orderings.clone(), orderings.clone());
    }

    final_orderings
}

/*
 * input: AST, valid lock identifiers
 * output: list of lock orderings, [[a,b], [b,a], etc...]
 * 
 * Panic situations:
 * 
 * 1) unsafe code block found
 * 2) mutex or sync primitive used without valid identifier
 * 3) New mutex is created
 * 
 */
fn parse_ast(expr: syn::Expr,
             lock_subst: HashMap<String, String>,
             orderings: Vec<Vec<String>>) -> Vec<Vec<String>> {
    //println!("{:?}", expr.clone());
    let result = match expr {
        Expr::Block(e) => {
            parse_stmts_vec(e.block.stmts, lock_subst.clone(), orderings.clone())
        },
        Expr::If(e) => {
            let mut ret: Vec<Vec<String>> = vec![];
            let cond = e.cond;
            let br1 = e.then_branch.stmts;
            let br2 = e.else_branch;

            let r1 = parse_ast(*cond, lock_subst.clone(), orderings.clone());
            let r2 = parse_stmts_vec(br1, lock_subst.clone(), orderings.clone());
            let r3 = match br2 {
                Some((_, expr)) => {
                    parse_ast(*expr, lock_subst.clone(), orderings.clone())
                }
                None => {
                    vec![]
                },
            };

            let mut r4: Vec<Vec<String>> = r1.clone();
            r4.extend(r2.clone());

            let mut r5: Vec<Vec<String>> = r1.clone();
            r5.extend(r3.clone());
            ret.extend(r4);
            ret.extend(r5);
            ret
        },
        /*
         * Method calls are tricky, we want to ensure the following properties:
         * 1) We can allow a lock reference to be borrowed, but only 1 at a time
         * 2) We want to track all <identifier>.lock() function calls to extract
         *    lock orderings.
         * 3) When we do call <identifier>.lock(), we want to ensure that
         *    <identifier> is in the valid subset of static identifiers.
         * 4) We disallow indirect function calls. We accomplish this by tracking
         *    all let bindings, and preventing the receiver from being an identifier
         *    that was assigned to a local variable.
         * 5) We disallow Arc::clone from being used on lock references.
         */
        Expr::MethodCall(m) => {
            let receiver = m.clone().receiver;
            let mut r1 = parse_ast(*receiver.clone(), lock_subst.clone(), orderings.clone());
            let method_ident = m.method.to_string();

            /*
             * If we find a lock() call, add the identifier to the current set of lock
             * orderings.
             */
            if method_ident.clone() == "lock" {
                let lock_name = match *receiver {
                    Path(p) => {
                        let path = String::from(&p.path.segments[0].ident.to_string());
                        path
                    },
                    _ => String::from("")
                };

                if lock_subst.contains_key(&lock_name.clone()) {
                    r1.push(vec![lock_name.clone()]);
                }
            }
            r1
        },
        Expr::While(while_expr) => {
            let cond = while_expr.cond;
            let body = while_expr.body;

            let mut r1 = parse_ast(*cond, lock_subst.clone(), orderings.clone());
            let r2 = parse_stmts_vec(body.stmts, lock_subst.clone(), orderings.clone());

            r1.extend(r2);

            r1
        },
        Expr::Loop(l) => {
            let body = l.body;
            parse_stmts_vec(body.stmts, lock_subst.clone(), orderings.clone())
        },
        Expr::ForLoop(fl) => {
            let expr = fl.expr;
            let body = fl.body;
            
            let mut r1 = parse_ast(*expr, lock_subst.clone(), orderings.clone());
            let r2 = parse_stmts_vec(body.stmts, lock_subst.clone(), orderings.clone());

            r1.extend(r2);

            r1
        },
        Expr::Call(c) => {
            println!("{:?}", c.clone());
            let mut arg_lst: Vec<Vec<String>> = vec![];
            let mut arg_ctr = 0;
            /*
             * Here we restrict the usage of Arc::clone, to prevent lock references
             * from being bound to new locals. Arc::clone is the only way to clone
             * an Arc pointer.
             */
            match *(c.clone().func) {
                Expr::Path(p) => {
                    /*
                     * We need to check if any two adjacent elements of segments are equal to
                     * Arc::clone because "::std::sync::Arc::clone(ident);" is also valid.
                     */
                    if p.path.segments.len() >= 2 {
                        for idx in 0..p.path.segments.len() - 1 {
                            if p.path.segments[idx].ident == "Arc" &&
                            p.path.segments[idx+1].ident  == "clone" {
                                panic!("Calling Arc::clone inside threads! is not permitted")
                            }
                        }
                    }
                },
                _ => panic!("Expr::call func is not a ExprPath"),
            };

            /*
             * Here we want to enforce the following property
             * 1) We want to ensure that only 1 lock reference can be passed down into a function
             *    at any given time.
             * 
             *    We can accomplish this by searching each lock argument for scanning the argument
             *    list for lock identifiers, and ensuring only 1 argument has a valid lock identifier.
             * 
             *    We prevent the copying of lock references using Arc::clone.  In addition, when
             *    parsing statements, we prevent lock references from being assigned to new 'let'
             *    bindings. The combination of these two actions are what allow us to perform 
             */

            // first, check the arguments for Arc::clone 
            for argument in c.clone().args {
                // traverse the argument AST, add each lock
                let result = parse_ast(argument.clone(), lock_subst.clone(), orderings.clone());
                arg_lst.extend(result.clone());

                /*
                 * If one argument returns more than one lock identifier, we will overapproximate
                 * and assume that more than one lock reference is being passed down
                 */
                if result.len() > 1 {
                    panic!("More than one lock acquired in function arguments!");
                }
                if result.len() > 0 {
                    arg_ctr += 1;
                }
                
            }

            // Only 1 argument should ever contain a lock reference!
            if arg_ctr > 1 {
                panic!("More than one lock acquired in function arguments!");
            }

            // if the arguments all check out, we can just return 
            let mut r1 = parse_ast(*c.func, lock_subst.clone(), orderings.clone());
            r1.extend(arg_lst);
            r1
        }
        Expr::Match(m) => {
            // TODO: need to parse each match arm!!!
            let mut return_lst: Vec<Vec<String>> = vec![];
            let r1 = parse_ast(*m.expr, lock_subst.clone(), orderings.clone());
            for arm in m.arms {
                return_lst.extend(parse_ast(*arm.body, lock_subst.clone(), orderings.clone()));
            }
            extend_orderings(r1, return_lst)
        },
        Expr::Closure(closure) => {
            let body = closure.body;
            parse_ast(*body, lock_subst.clone(), orderings.clone())
        },
        Expr::Lit(l) => {
            println!("{:?}", l);
            vec![]
        },
        Expr::Paren(p) => {
            parse_ast(*p.expr, lock_subst.clone(), orderings.clone())
        },
        Expr::AssignOp(a_op) => {
            /*
             * https://doc.rust-lang.org/reference/expressions.html
             * AssignOps such as '+=' are evaluated right-to-left
             */
            let mut r = parse_ast(*a_op.right, lock_subst.clone(), orderings.clone());
            let l = parse_ast(*a_op.left, lock_subst.clone(), orderings.clone());
            r.extend(l);
            r
        },
        Expr::Path(p) => {
            println!("{:?}", p);
            vec![]
        },
        Expr::Reference(r) => {
            parse_ast(*r.expr, lock_subst.clone(), orderings.clone())
        },
        Expr::Unary(u) => {
            parse_ast(*u.expr, lock_subst.clone(), orderings.clone())
        },
        _ => {
            panic!("unsupported expression found!: {:?}", expr);
        }
    };
    println!("{:?}", result);
    let mut return_val = orderings.clone();
    return_val.extend(result);
    return_val.to_vec()
}

// in your proc-macro crate
#[proc_macro]
pub fn threads(input: TokenStream) -> TokenStream {
    let mut ret: Vec<TokenStream2> = vec![];
    let mut locks: HashMap<String, String> = HashMap::new();
    let mut orderings = vec![];

    let mut block_preamble: Option<TokenStream2> = None;
    let mut count = 0;
    let mut thread_joins: Vec<TokenStream2> = vec![];


    // parse input to extract lock declaration
    let re = Regex::new(r"\{\s*locks\s*=\s*\{(.*?)\}").unwrap();
    let data = Regex::new(r"\((.*?)\)").unwrap();
    let identifier_regex = Regex::new(r"(.*?)\s*\(").unwrap();
    for a in input.clone().into_iter() {
        // if lock decl block, add lock values to block
        if re.is_match(&a.to_string()) {
            let a_str = &a.to_string(); 
            let lock_names = re.captures(a_str).unwrap()
                               .get(1).map_or("", |m| m.as_str()); 
            for s in lock_names.split(",") {
                if data.is_match(&s.trim()) && s.trim() != "" {
                    let data = data.captures(s.trim()).unwrap()
                                    .get(1).map_or("", |m| m.as_str()); 
                    let final_ident = identifier_regex.captures(s.trim()).unwrap()
                                                       .get(1).map_or("", |m| m.as_str());
                    locks.insert(String::from(final_ident), String::from(data));
                } else {
                    panic!("Invalidly formatted lock declaration section: locks = {identifier(data)...}");
                }
            }

            // now create the global block preamble
            /*
                    let a = Arc::new(Mutex::new(0));
                    let b = Arc::new(Mutex::new(0));
                    let c = Arc::new(Mutex::new(0));
                {locks = {a, b, c}}, {
                    etc....
             */

            let mut temp_vec = vec![];
            // order doesn't matter here
            for (k, v) in &locks {
                let varname = format_ident!("_threads_macro_{}", k);
                let data = syn::parse_str::<Expr>(&v.to_string()).unwrap();

                let q = quote! {
                    let #varname = ::std::sync::Arc::new(::std::sync::Mutex::new(#data));
                };
                temp_vec.push(q);
            }
            

            block_preamble = Some(quote! { #(#temp_vec)* } );

        // else if we have the ',' token
        } else if &a.to_string() == "," {
            dbg!("comma found!");
            // pass
        // else, parse each thread block
        } else {
            dbg!("parsing thread block!: {:?}", &a.to_string());

            // for each thread block, extract lock orderings & conditions
            let expr = syn::parse_str::<Expr>(&a.to_string()).unwrap();
            // parse the AST
            let temp: Vec<Vec<String>> = vec![];
            orderings.extend(parse_ast(expr.clone(), locks.clone(), temp));
            match block_preamble.clone() {
                Some(_) => {

                    // for each lock, we need to clone it
                    let mut cloned_lock_builder = vec![];
                    let mut internal_lock_builder = vec![];
                    for (k, _) in &locks {
                        let varname = format_ident!("_thread_{}_lock_{}", count.to_string(), k);
                        let lockid = format_ident!("_threads_macro_{}", k);
                        let q = quote! {
                            let #varname = ::std::sync::Arc::clone(&#lockid);
                        };

                        let internal_varname = format_ident!("{}", k);

                        let q2 = quote! {
                            let #internal_varname = #varname;
                        };

                        cloned_lock_builder.push(q);
                        internal_lock_builder.push(q2);
                    }

                    let varname = format_ident!("_thread_{}", count.to_string());
                    let q = quote! {
                        #(#cloned_lock_builder)*
                        let #varname = ::std::thread::spawn(move || {
                            #(#internal_lock_builder)*
                            #expr
                        });
                    };
                    ret.push(q);
                    thread_joins.push(quote! {
                        #varname.join(); 
                    } );
                    count += 1;
                },
                None => panic!("Improperly called threads! macro, must specify locks as first parameter"),
            };
        }
    }

    // merge all lock orderings and conditions + solve in z3
    dbg!(orderings.clone());

    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    
    let result = solve_constraints(ctx, orderings);
    match result {
        z3::SatResult::Sat => {
            println!("No deadlocks are present");
        },
        z3::SatResult::Unsat => {
            panic!("Potential Deadlock detected");
        },
        z3::SatResult::Unknown => {
            panic!("SAT solver returned unknown -- potential deadlock detected!");
        }
    }


    let preamble = match block_preamble {
        Some(p) => p,
        None => panic!("Unable to generate block preamble - check macro formatting"),
    };

    let mut export_lock_builder = vec![];
    for (k, _) in &locks {
        let varname = format_ident!("_export_threads_lock_{}", k);
        let lockid = format_ident!("_threads_macro_{}", k);
        let export_var_name = format_ident!("{}", k);

        let q = quote! {
            let #varname = ::std::sync::Arc::clone(&#lockid);
            let #export_var_name = #varname;
        };
        export_lock_builder.push(q);
    }

    // return output
    let f = quote! {
        #preamble
        #(#ret)*
        #(#thread_joins)*
        #(#export_lock_builder)*
    };

    f.into()
}