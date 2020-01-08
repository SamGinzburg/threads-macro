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
use syn::Expr::*;
use syn::Stmt::*;
use std::collections::HashMap;
use z3::{Config, Context, Solver};
use solver::solve_constraints;
use quote::{quote, format_ident};
use syn::export::TokenStream2;
use std::sync::{Arc, Mutex};
use std::thread;

fn coalesce(input: Vec<Vec<String>>) -> Vec<String> {
    let mut ret: Vec<String> = vec![];
    for v in &input {
        for nested in v.clone() {
            ret.push(nested.clone());
        }
    }
    ret
}

fn parse_stmts_vec(stmts: Vec<syn::Stmt>,
                   lock_subst: HashMap<String, String>,
                   in_lst: Vec<String>) -> Vec<Vec<String>> {
    let mut ret: Vec<Vec<String>> = vec![];
    let subst = lock_subst.clone();
    let temp_lst = in_lst.clone();
    let mut locals = vec![];
    let mut expr_lst = vec![];
    for statement in stmts {
        //dbg!("{:?}", statement.clone());
        let temp = match statement {
            Local(l) => {
                let pat = l.clone().pat;
                let init = l.clone().init;

                // TODO: check for assignment of locks here

                dbg!(pat.clone());
                dbg!(init.clone());

                let r1 = match init.clone() {
                    Some((_, e)) => {
                        parse_ast(*e, subst.clone(), temp_lst.clone())
                    },
                    None => {
                        vec![]
                    }
                };
                locals.extend(r1);
            },
            Item(i) => panic!("item"),
            Expr(nested_e) => {
                let r1 = parse_ast(nested_e, subst.clone(), temp_lst.clone());
                expr_lst.extend(r1);
            },
            Semi(nested_e, _) => {
                let r1 = parse_ast(nested_e, subst.clone(), temp_lst.clone());
                expr_lst.extend(r1);
            },
        };
        dbg!("ret in vec stmts {:?}", ret.clone());
    }

    if locals.len() > 0 {
        let ord = coalesce(locals);
        ret.extend(vec![ord]);
    }

    if expr_lst.len() > 0 {
        ret.extend(expr_lst);
    }

    ret
}

/*
 * input: AST, valid lock identifiers
 * output: list of lock orderings, [[a,b], [b,a], etc...]
 * 
 * Panic situations:
 * 
 * 1) unsafe code block found (DONE)
 * 2) mutex or sync primitive used without valid identifier
 * 3) New mutex is created
 * 
 */
fn parse_ast(expr: syn::Expr,
             lock_subst: HashMap<String, String>,
             orderings: Vec<String>) -> Vec<Vec<String>> {
    /*println!("start parsing: {:?}\t{:?}\t{:?}", expr.clone(),
                                                lock_subst.clone(),
                                                orderings.clone()); */
    let mut ret = vec![orderings.clone()];
    let lst = match expr {
        Expr::Block(e) => {
            //dbg!("block found! {:?}", e);
            parse_stmts_vec(e.block.stmts, lock_subst.clone(), orderings.clone())
        },
        Expr::If(e) => {
            let cond = e.cond;
            let br1 = e.then_branch;
            let br2 = e.else_branch;

            let mut r1 = parse_ast(*cond, lock_subst.clone(), orderings.clone());
            
            let r2 = parse_stmts_vec(br1.stmts, lock_subst.clone(), orderings.clone());

            let r3 = match br2 {
                Some((_, box e)) => parse_ast(e, lock_subst.clone(), orderings.clone()),
                None => {
                    dbg!("parsed else token");
                    vec![]
                },
            };
        
            let r2_coalesced = coalesce(r2.clone());
            let r3_coalesced = coalesce(r3.clone());

            dbg!("r2:\t{:?}", r2_coalesced.clone());
            dbg!("r3:\t{:?}", r3_coalesced.clone());

            if r2_coalesced.len() > 0 {
                r1.insert(r1.len(), r2_coalesced);
            }

            if r3_coalesced.len() > 0 {
                r1.insert(r1.len(), r3_coalesced);
            }
            dbg!("if:\t{:?}", r1.clone());
            r1
        },
        Expr::While(while_expr) => {
            let cond = while_expr.cond;
            let body = while_expr.body;

            let mut r1 = parse_ast(*cond, lock_subst.clone(), orderings.clone());
            let r2 = parse_stmts_vec(body.stmts, lock_subst.clone(), orderings.clone());

            if r2.len() > 0 {
                r1.extend(r2);
            }

            r1
        },
        Expr::Loop(l) => {
            let body = l.body;
            let r1 = parse_stmts_vec(body.stmts, lock_subst.clone(), orderings.clone());

            let r1_coalesced = coalesce(r1.clone());

            //dbg!("loop:\t{:?}", r1_coalesced.clone());

            vec![r1_coalesced]
            //vec![]
        },
        Expr::ForLoop(fl) => {
            let expr = fl.expr;
            let body = fl.body;
            
            let mut r1 = parse_ast(*expr, lock_subst.clone(), orderings.clone());
            let r2 = parse_stmts_vec(body.stmts, lock_subst.clone(), orderings.clone());

            if r2.len() > 0 {
                r1.extend(r2);
            }
            r1
        },
        Expr::Match(m) => {
            let expr = m.expr;
            parse_ast(*expr, lock_subst.clone(), orderings.clone())
        },
        Expr::Closure(closure) => {
            let body = closure.body;
            parse_ast(*body, lock_subst.clone(), orderings.clone())
        },
        Expr::MethodCall(m) => {
            let receiver = m.clone().receiver;

            let mut r1 = parse_ast(*receiver.clone(), lock_subst.clone(), orderings.clone());

            let method_ident = m.method.to_string();

            if method_ident.clone() == "lock" {
                let lock_name = match *receiver {
                    Path(p) => {
                        let path = String::from(&p.path.segments[0].ident.to_string());
                        path
                    },
                    _ => String::from("")
                };
                // check for a valid lock identifier here, if not valid, panic
                if lock_subst.contains_key(&lock_name.clone()) {
                    // if the lock is valid, we need to append to the list of valid lock orderings
                    let mut tmp = orderings.clone();
                    let mut tmp_vec = vec![lock_name.clone()];
                    //tmp.append(&tmp_vec);
                    dbg!("{:?}", r1.clone());
                    r1.insert(r1.len(), vec![String::from(lock_name.clone())]);
                } else {
                    panic!("Invalid lock acquired: {:?}", m.clone());
                }
            }
            
            r1
        },/*
        Expr::ExprMatch(exprmatch) => {
            let expr1 = exprmatch.expr;

            let mut r1 = parse_ast(expr1, lock_subst.clone(), orderings.clone());

            for arm in exprmatch.arms {
                let body = *arm.body;
                let r2 = parse_ast(body, lock_subst.clone(), orderings.clone());
                if r2.len() > 0 {
                    r1.extend(r2);
                }
            }

            r1
        }, */
        Expr::Unsafe(_) => {
            panic!("unsafe block inside threads macro is not permitted!");
        },
        /*
        We do not handle the following cases:

        Macros - we do not expand macros here

        */
        _ => {
            //dbg!("other!");
            vec![]
        },
    };

    /*
    let mut tmp = vec![];
    for sub_lst in lst.clone() {
        dbg!("{:?}", sub_lst.clone());
        if sub_lst.clone().len() > 0 {
            tmp.extend(Vec::from(sub_lst));
        }
    }
    dbg!("{:?}", tmp.clone());
    dbg!("{:?}", ret.clone());

    vec![tmp]
    */

    ret.extend(lst);
    ret
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
        dbg!("capture group: {}", &a);
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
                    dbg!("{}", final_ident);
                    dbg!("{}", data);
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

                // this goes at the very top of the macro
                //dbg!("{:?}", varname.clone());
                //dbg!("{:?}", data.clone());

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
            //let ord = coalesce(parse_ast(expr.clone(), locks.clone(), vec![]));
            orderings.extend(parse_ast(expr.clone(), locks.clone(), vec![]));
            let preamble = match block_preamble.clone() {
                Some(p) => {

                    // for each lock, we need to clone it
                    let mut cloned_lock_builder = vec![];
                    let mut internal_lock_builder = vec![];
                    for (k, v) in &locks {
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
    for (k, v) in &locks {
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