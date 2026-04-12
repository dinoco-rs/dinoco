// File automatically generated
// Do not manually change it

use tuono_lib::{tokio, Mode, Server, axum::Router, tuono_internal_init_v8_platform};
use tuono_lib::axum::routing::get;


const MODE: Mode = Mode::Dev;

// MODULE_IMPORTS
#[path="../src/routes/index.rs"]
                    mod index;
                    


#[tokio::main]
async fn main() {
    tuono_internal_init_v8_platform();
    
    if MODE == Mode::Prod {
        println!("\n  ⚡ Tuono v0.19.7");
    }

    

    let router = Router::new()
        // ROUTE_BUILDER
.route("/", get(index::tuono_internal_route)).route("/__tuono/data/", get(index::tuono_internal_api))        ;

    Server::init(router, MODE).await.start().await
}

