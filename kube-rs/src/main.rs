
use k8s_openapi::api::core::v1::Pod;

use kube::{
    api::{Api, DeleteParams, ListParams, Meta, PatchParams, PostParams, WatchEvent},
    Client,
};

use futures::executor::block_on;



fn main() {
    println!("Hello, world!");

    std::env::set_var("RUST_LOG", "info,kube=debug");


    let client = block_on( Client::try_default() ).unwrap();


    let namespace = std::env::var("NAMESPACE").unwrap_or("default".into());


    let pods: Api<Pod> = Api::namespaced(client, &namespace);





}
