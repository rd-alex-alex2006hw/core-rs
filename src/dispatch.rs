//! Dispatch takes messages sent from our wonderful UI and runs the needed core
//! code to generate the response. Essentially, it's the RPC endpoint for core.
//!
//! Each message sent in is in the following format (JSON):
//! 
//!     ["<message id>", "<command>", arg1, arg2, ...]
//!
//! where the arg\* can be any valid JSON object. The Message ID is passed in
//! when responding so the client knows which request we are responding to.

use ::futures::Future;
use ::jedi::{self, Value};
use ::config;

use ::error::{TResult, TError};
use ::util;
use ::util::event::Emitter;
use ::turtl::TurtlWrap;
use ::models::user::User;

/// process a message from the messaging system. this is the main communication
/// heart of turtl core.
pub fn process(turtl: TurtlWrap, msg: &String) -> TResult<()> {
    let data: Value = try!(jedi::parse(msg));

    // grab the request id from the data
    let mid: String = match jedi::get(&["0"], &data) {
        Ok(x) => x,
        Err(_) => return Err(TError::MissingField(String::from("missing mid (0)"))),
    };
    // grab the command from the data
    let cmd: String = match jedi::get(&["1"], &data) {
        Ok(x) => x,
        Err(_) => return Err(TError::MissingField(String::from("missing cmd (1)"))),
    };

    info!("dispatch({}): {}", mid, cmd);
    match cmd.as_ref() {
        "user:login" => {
            let username = try!(jedi::get(&["2", "username"], &data));
            let password = try!(jedi::get(&["2", "password"], &data));
            let turtl1 = turtl.clone();
            let turtl2 = turtl.clone();
            let mid = mid.clone();
            let mid2 = mid.clone();
            User::login(turtl.clone(), &username, &password)
                .map(move |_| {
                    debug!("dispatch({}) -- user:login success", mid);
                    match turtl1.msg_success(&mid, jedi::obj()) {
                        Err(e) => error!("dispatch -- problem sending login message: {}", e),
                        _ => ()
                    }
                })
                .map_err(move |e| {
                    turtl2.api.clear_auth();
                    match turtl2.msg_error(&mid2, &e) {
                        Err(e) => error!("dispatch -- problem sending login message: {}", e),
                        _ => ()
                    }
                })
                .forget();
            Ok(())
        },
        "user:logout" => {
            try!(User::logout(turtl.clone()));
            util::sleep(1000);
            turtl.msg_success(&mid, jedi::obj())
        },
        "user:join" => {
            turtl.msg_success(&mid, jedi::obj())
        },
        "app:start-sync" => {
            try!(turtl.start_sync());
            let turtl2 = turtl.clone();
            turtl.events.bind_once("sync:incoming:init", move |err| {
                // using our crude eventing system, a bool signals a success, a
                // string is an error (containing the error message)
                match *err {
                    Value::Bool(_) => {
                        try_or!(turtl2.msg_success(&mid, jedi::obj()), e,
                            error!("dispatch -- app:start-sync: error sending success: {}", e));
                    },
                    Value::String(ref x) => {
                        try_or!(turtl2.msg_error(&mid, &TError::Msg(x.clone())), e,
                            error!("dispatch -- app:start-sync: error sending error: {}", e));
                    },
                    _ => {
                        error!("dispatch -- unknown sync error: {:?}", err);
                        try_or!(turtl2.msg_error(&mid, &TError::Msg(String::from("unknown error initializing syncing"))), e,
                            error!("dispatch -- app:start-sync: error sending error: {}", e));
                    },
                }
            }, "dispatch:sync:init");
            Ok(())
        },
        "app:pause-sync" => {
            turtl.events.trigger("sync:pause", &jedi::obj());
            turtl.msg_success(&mid, jedi::obj())
        },
        "app:resume-sync" => {
            turtl.events.trigger("sync:resume", &jedi::obj());
            turtl.msg_success(&mid, jedi::obj())
        },
        "app:shutdown-sync" => {
            turtl.events.trigger("sync:shutdown", &jedi::obj());
            turtl.msg_success(&mid, jedi::obj())
        },
        "app:api:set-endpoint" => {
            let endpoint: String = try!(jedi::get(&["2"], &data));
            try!(config::set(&["api", "endpoint"], &endpoint));
            turtl.msg_success(&mid, jedi::obj())
        },
        "app:shutdown" => {
            info!("dispatch: got shutdown signal, quitting");
            match turtl.msg_success(&mid, jedi::obj()) {
                Ok(..) => (),
                Err(..) => (),
            }
            util::sleep(10);
            turtl.events.trigger("app:shutdown", &jedi::to_val(&()));
            Ok(())
        },
        "ping" => {
            info!("ping!");
            turtl.msg_success(&mid, Value::String(String::from("pong")))
        },
        _ => {
            match turtl.msg_error(&mid, &TError::MissingCommand(cmd.clone())) {
                Err(e) => error!("dispatch -- problem sending error message: {}", e),
                _ => ()
            }
            Err(TError::Msg(format!("bad command: {}", cmd)))
        }
    }
}

