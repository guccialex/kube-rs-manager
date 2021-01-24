#![feature(proc_macro_hygiene, decl_macro)]
#[macro_use] extern crate rocket;


use kube::{
    api::{Api, DeleteParams, ListParams, Meta, PostParams, WatchEvent},
    Client,
};


use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::api::core::v1::Service;
use k8s_openapi::api::core::v1::Node;


use tungstenite::{Message};
use tungstenite::handshake::server::{Request, Response};
use tungstenite::accept_hdr;


use std::net::TcpListener;
use std::net::TcpStream;
use std::process::Command;
use std::{thread, time};
use std::sync::Arc;
use std::sync::Mutex;



struct GlobalValues{


}

impl GlobalValues{

    fn get_gamepod_image() -> String{

        "gcr.io/level-unfolding-299521/github.com/guccialex/ccp-websocket-server@sha256:ad64221569320fd357787f244b16ccb3856761483f58264c39898e558a9379a1".to_string()
    }
}




use rocket::State;


#[tokio::main]
async fn main() {
    
    std::env::set_var("RUST_LOG", "info,kube=debug");
    
    
    let namespace = std::env::var("NAMESPACE").unwrap_or("default".into());
    
    //connect to the kubernetes pod and service apis
    let client = Client::try_default();
    let client2 = Client::try_default();
    let client3 = Client::try_default();
    
    
    let client = client.await.unwrap();//block_on(client).unwrap();
    let podapi: Api<Pod> = Api::namespaced(client, &namespace);
    
    let client2 = client2.await.unwrap();
    let serviceapi: Api<Service> = Api::namespaced(client2, &namespace);
    
    let client3 = client3.await.unwrap();
    let nodeapi: Api<Node> = Api::all(client3);
    
    
    
    let mutexmain = Arc::new(Mutex::new(Main::new(podapi, serviceapi, nodeapi)));
    
    
    
    //listen for clients who want to be assigned a game on port 8000
    let copiedmutexmain = mutexmain.clone();      
    
    
    tokio::spawn(async move {
        
        rocket::ignite()
        .manage(copiedmutexmain)
        .mount("/", routes![join_private_game, join_public_game, create_private_game, health_check ])
        .launch();
    });
    
    

    
    
    //a new thread that ticks the main every second
    let copiedmutexmain = mutexmain.clone();
    
    
    
    loop {
        println!("ticking");
        
        //every second
        let sleeptime = time::Duration::from_millis(1000);
        thread::sleep( sleeptime );
        
        //unlock the mutex main while handling this message
        
        {
            let mut main = copiedmutexmain.lock().unwrap();
            main.tick().await;
        }
    };
    
}


//try to create a new gamepod server
//with this id
async fn create_gamepod(podapi: & kube::Api<k8s_openapi::api::core::v1::Pod>, gamepodid: u32 ){
    
    println!("making game pod {:?}", gamepodid);
    
    
    let podname = "gamepod".to_string() + &gamepodid.to_string();
    
    
    
    // Create the pod
    let pod: Pod = serde_json::from_value(serde_json::json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": podname,
            
            "labels": {
                "podtype": "gameserver",
                "gameserverid": gamepodid.to_string(),
            },
        },
        "spec": {
            "containers": [{
                "name": "container",
                "image": GlobalValues::get_gamepod_image(),
            }],
        }
    })).unwrap();
    
    
    
    let postparams = PostParams::default();
    
    
    podapi.create(&postparams, &pod).await;
}



//create an external load balancer for this gamepodid
async fn create_external_load_balancer(serviceapi: & kube::Api<k8s_openapi::api::core::v1::Service>, gamepodid: u32 ) {
    
    println!("making load balancer {:?}", gamepodid);
    
    
    let servicename = "service".to_string()+ &gamepodid.to_string();
    let serviceid = gamepodid.to_string();
    
    
    
    let service: Service = serde_json::from_value(serde_json::json!({
        
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {
            //its name is "service231" where the number is the pod id
            "name": servicename,
            
            "labels": {
                "servicetype": "gamepodexposer",
                "serviceid": serviceid,
            },
        },
        "spec": {
            "type": "NodePort",
            
            //select the game pod with this id
            "selector": {
                "gameserverid": gamepodid.to_string(),
            },
            
            //expose port 4000
            "ports": [{
                "protocol": "TCP",
                //the port and target port mean the same thing right? what port to forward the connection on the nodeport
                "port": 4000,
                "targetPort": 4000,
            }],
            
        }
    })).unwrap();
    
    
    let postparams = PostParams::default();
    
    
    serviceapi.create( &postparams, &service).await;
}





//the function called when the player wants to join different games
//return the ip and port and password


//join public, join private, create private


#[get("/create_private_game")]
fn create_private_game( state: State<Arc<Mutex<Main>>> ) -> String {
    
    println!("request to join private game");
    
    let gametoconnectto = GameToConnectTo::createprivategame;
    
    let game = state.inner();
    let mut game = game.lock().unwrap();
    
    game.connect_to_game(gametoconnectto).to_string()
}


#[get("/join_public_game")]
fn join_public_game( state: State<Arc<Mutex<Main>>> ) -> String {
    
    println!("request to join public game");
    
    let gametoconnectto = GameToConnectTo::joinpublicgame;
    
    let game = state.inner();
    let mut game = game.lock().unwrap();
    
    game.connect_to_game(gametoconnectto).to_string()
}


#[get("/join_private_game/<password>")]
fn join_private_game( password: String, state: State<Arc<Mutex<Main>>> ) -> String {
    
    println!("request to join private game");
    
    let gametoconnectto = GameToConnectTo::joinprivategame(password);
    
    let game = state.inner();
    let mut game = game.lock().unwrap();
    
    game.connect_to_game(gametoconnectto).to_string()
}



use rocket::http::Status;

//respond to the health check and return a status of 200
#[get("/")]
fn health_check() -> Status{

    Status::Ok
}


use std::collections::{HashMap, HashSet};


struct Main{
    
    
    //the mapping of each pod to its internal IP
    podips: HashMap< u32, String >,
    
    //the pods that dont have a password set yet
    unallocatedpods: Vec<u32>,
    
    //the map of each password to the gamepods id
    openpodandpassword: HashMap<String, u32>,
    
    //the pod id to the external port its opened on
    podidtoexternalport: HashMap<u32, String>,
    
    //the externalIP of a node in this cluster 
    nodeexternalip: Option<String>,
    
    
    
    
    
    podapi: Api<Pod>,
    serviceapi: Api<Service>,
    nodeapi: Api<Node>,
    
}

use std::error;
use std::any::Any;

impl Main{
    
    
    fn new(podapi: Api<Pod>, serviceapi: Api<Service>, nodeapi: Api<Node>) -> Main{
        
        Main{
            
            podips: HashMap::new(),
            
            unallocatedpods: Vec::new(),
            openpodandpassword: HashMap::new(),
            
            podidtoexternalport: HashMap::new(),
            
            nodeexternalip: None,
            
            podapi: podapi,
            serviceapi: serviceapi,
            nodeapi: nodeapi,
            
        }
    }
    
    
    //set the password of an unallocated pod, and return the id of the pod set
    fn get_unallocated_pod_and_set_password(&mut self, maybepassword: Option<&str>) -> Result<(u32, String), Box<error::Error> >{
        

        let podid = self.unallocatedpods.pop().ok_or("error")?;
        
        let podip = self.podips.remove(&podid).ok_or("error")?;
        

        let passwordtoset: String;

        //if theres a specific password thats want to be set
        if let Some(maybepassword) = maybepassword{

            passwordtoset = maybepassword.to_string();
        }
        else{

            use rand::{distributions::Alphanumeric, Rng};
        
            passwordtoset = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(7)
            .map(char::from)
            .collect();
        }

        
        
        let address = "http://".to_string() + &podip.clone() + ":8000";
        
        let resp = reqwest::blocking::get(  &(address.to_string() + "/set_password/"+ &passwordtoset)  );
        
        
        //return podid;
        return Ok((podid, passwordtoset));
    }
    
    
    //tell the pod with this ID that a player was allocated to it
    fn tell_pod_player_was_allocated(&mut self, podid: u32){
        
        if let Some(podip) = self.podips.get(&podid){
            
            let address = "http://".to_string() + &podip.clone() + ":8000";
            
            if let Ok(result) = reqwest::blocking::get( &(address.clone() + "/assign_player") ){
            }
        }
    }
    
    
    //a player who wants to connect to a game
    //join a game and return the address and port and password
    //as a JSON value
    fn connect_to_game(&mut self, gametoconnectto: GameToConnectTo) -> String{
        
        
        let thepassword: String;
        let thepodid: u32;
        
        
        if let GameToConnectTo::joinprivategame(password) = gametoconnectto{
            
            
            //if a pod with that password exists and is open, return it 
            if let Some(podid) = self.openpodandpassword.remove(&password){
                
                thepodid = podid;
                thepassword = password;
            }
            else{
                //this isnt a valid "game to connect to"
                //so return to the client requesting, "no" to let them know its invalid
                
                return "no".to_string();
            }
            
        }
        else if let GameToConnectTo::joinpublicgame = gametoconnectto{
            
            let password = "password".to_string();
            
            //if a pod with that password exists and is open, return it 
            if let Some(podid) = self.openpodandpassword.remove(&password){
                
                thepodid = podid;
                thepassword = password;
                
            }
            //otherwise, set the password of an unallocated pod to that password
            //and return that pod
            else{
                
                if let Ok( (podid, password) ) = self.get_unallocated_pod_and_set_password( Some("password") ){
                    thepodid = podid;
                    thepassword = password;
                }
                else{
                    return "no".to_string();
                }
                
                
            }
        }
        else if let GameToConnectTo::createprivategame = gametoconnectto{
            
            if let Ok( (podid, password) ) = self.get_unallocated_pod_and_set_password( None ){
                thepodid = podid;
                thepassword = password;
            }
            else{
                return "no".to_string();
            }
            
        }
        else{
            panic!("hmm");
        }
        
        
        
        //if theres a valid external node ip and a valid external nodeport for the pod
        if let Some(nodeexternalip) = self.nodeexternalip.clone(){
            if let Some(externalport) = self.podidtoexternalport.get(&thepodid){
                
                let externalport = &externalport.clone();
                
                //before returning, send a message to the pod, that a player has just been allocated to it
                self.tell_pod_player_was_allocated(thepodid);
                
                
                let addressandport = "http://".to_string() + &nodeexternalip + ":" + externalport;
                
                let connectedtogame = ConnectedToGame{
                    addressandport: addressandport,
                    gamepassword: thepassword,
                };
                
                let toreturn = serde_json::to_string(&connectedtogame).unwrap();
                
                return toreturn;
            }
        }
        

        return "no".to_string();        
    }
    
    
    
    async fn tick(&mut self){


        self.podips = HashMap::new();
        self.unallocatedpods = Vec::new();
        self.openpodandpassword = HashMap::new();
        self.podidtoexternalport = HashMap::new();
        self.nodeexternalip = None;


        
        
        
        //a list of every pod with an ip mapped by its ID
        let mut podswithips: HashMap<u32, String> = HashMap::new();
        let mut allgamepods = HashSet::new();
        
        //get the list of every pod with an ID and IP
        {
            
            let lp = ListParams::default()
            .timeout(1)
            .labels( &("podtype=gameserver") );
            
            
            let result = self.podapi.list(&lp).await.unwrap();
            
            
            for item in result{
                
                let id = item.metadata.labels.clone().unwrap().get("gameserverid").unwrap().clone();
                
                let id = id.parse::<u32>().unwrap();
                
                allgamepods.insert(id.clone());
                
                
                if let Some(ips) = item.status.clone(){
                    
                    if let Some(ip) = ips.pod_ip{
                        
                        podswithips.insert(id, ip);
                    }
                }
            }
        }
        
        //for every pod id lacking, create that pod
        for x in 0..5{
            
            if allgamepods.contains(&x){
                continue;
            }
            else{
                create_gamepod(&self.podapi, x).await;
            }            
        }
        
        
        
        
        
        
        
        self.podips = podswithips;
        
        
        //for every pod with an ip
        //get its state        
        for (podid, podip) in &self.podips{
            
            //self.podips.insert(podid, podip);
            
            let address = "http://".to_string() + &podip.clone() + ":8000";
            
            println!("calling the pod with an IP to get its state {:?}", address);
            
            if let Ok(result) = reqwest::get( &(address.clone() + "/get_state") ).await{
                
                let body = result.text().await.unwrap();
                
                
                if let Ok(statusnumber) = body.parse::<u32>(){
                    
                    //if the password isnt set
                    if statusnumber == 1{
                        
                        self.unallocatedpods.push( *podid);
                        
                    }
                    //if the password is set, and there are players left to be assigned
                    else if statusnumber == 2{
                        
                        let password = reqwest::get( &(address + "/get_password") )
                        .await
                        .unwrap()
                        .text()
                        .await
                        .unwrap();
                        
                        self.openpodandpassword.insert(password, *podid);
                        
                    }
                    else if statusnumber == 3{
                    }
                    else{
                    }
                }
                
                println!("status body result {:?}", body);
            }
            else{
                println!("no pod response");
            } 
            
            
        }
        
        
        
        self.nodeexternalip = None;
        
        //get the address of a random node that i can send back to the client
        //to route the connection through the nodeport to the pod through
        {
            
            
            let lp = ListParams::default()
            .timeout(2);
            
            
            let result = self.nodeapi.list(&lp).await.unwrap();
            
            
            //for the node
            for item in &result{
                
                if let Some(status) = &item.status{
                    
                    if let Some(addresses) = &status.addresses{
                        
                        //for each address
                        for address in addresses{
                            
                            if address.type_ == "ExternalIP"{
                                
                                self.nodeexternalip = Some(address.address.clone());
                            }
                        }
                    }
                }
            }
            
        }
        
        
        
        
        //get every exposer
        
        let mut exposers: HashSet<u32> = HashSet::new();
        
        //get the active node balancers
        //and use it to set the pods by ID to their exposed nodeport
        
        {
            let lp = ListParams::default()
            .timeout(2)
            .labels( "servicetype=gamepodexposer" );
            
            let result = self.serviceapi.list(&lp).await.unwrap();
            
            //the list of all exposers
            for item in &result{
                
                if let Some(labels) = &item.metadata.labels{
                    
                    //the serve by the id of the service, that is associated with the pod id
                    if let Some(exposerid) = labels.get("serviceid"){
                        
                        
                        
                        let exposerid = exposerid.parse::<u32>().unwrap();
                        exposers.insert( exposerid );
                        
                        
                        
                        if let Some(specs) = &item.spec{
                            
                            if let Some(port) = &specs.ports{
                                
                                if let Some(nodeport) = &port[0].node_port{
                                    
                                    self.podidtoexternalport.insert(exposerid, nodeport.to_string() );
                                }
                            }
                        }
                    }
                }
            }
        }
        
        
        
        
        
        
        
        //make the load balancers that dont exist
        for x in 0..5{
            
            if exposers.contains(&x){
                continue;
            }
            else{
                create_external_load_balancer( &self.serviceapi, x).await;
            }
        }



        //println!("the open pods and password {:?}", self.openpodandpassword);
        //println!("the unallocated pods {:?}", self.unallocatedpods);
        
        
        
    }
    
}









use serde::{Serialize, Deserialize};



//a request for how the client wants to join a game
#[derive(Serialize, Deserialize, Debug)]
pub enum GameToConnectTo{
    
    joinpublicgame,
    joinprivategame(String),
    createprivategame,
}



//the message sent when a client is connected to a game on the server
//and the game is active
#[derive(Serialize, Deserialize)]
pub struct ConnectedToGame{
    
    //the IP and port of the game
    addressandport: String,
    
    //the password of the game
    gamepassword: String,
}