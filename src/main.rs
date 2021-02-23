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
        "gcr.io/level-unfolding-299521/ccp-websocket-server:latest".to_string()
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
        .mount("/", routes![health_check, join_game, get_available_games ])
        .launch();
    });
    
    
    
    
    
    //a new thread that ticks the main every second
    let copiedmutexmain = mutexmain.clone();
    
    
    
    loop {
        println!("ticking");
        
        //every second
        let sleeptime = time::Duration::from_millis(3000);
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


#[get("/get_available_games")]
fn get_available_games( state: State<Arc<Mutex<Main>>> ) -> String{
    
    println!("requesting to see the games available");

    let game = state.inner();
    let mut game = game.lock().unwrap();
    
    game.get_available_games()
}


#[get("/join_game/<gameid>")]
fn join_game( gameid: u32, state: State<Arc<Mutex<Main>>> ) -> String {
    
    println!("request to join game");
    
    let game = state.inner();
    let mut game = game.lock().unwrap();
    
    game.connect_to_game(gameid)
}




use rocket::http::Status;


//respond to the health check and return a status of 200
#[get("/")]
fn health_check() -> Status{
    
    println!("health check performed");
    
    Status::Ok
}







use std::collections::{HashMap, HashSet};


struct Main{
    
    
    //every tick this is updated
    
    //pods to the number of players allocated
    //internal port
    //external port
    //and password
    pods: HashMap<u32, (u8, String, String, String)>,
    
    
    //how many pods this game should have
    podstomake: u32,
    
    
    
    
    //the externalIP of a node in this cluster 
    nodeexternalip: Option<String>,
    podapi: Api<Pod>,
    serviceapi: Api<Service>,
    nodeapi: Api<Node>,
    
}

use std::error;
//use std::any::Any;

impl Main{

    
    
    fn new(podapi: Api<Pod>, serviceapi: Api<Service>, nodeapi: Api<Node>) -> Main{        
        
        Main{
            pods: HashMap::new(),

            podstomake: 10,
            
            nodeexternalip: None,
            podapi: podapi,
            serviceapi: serviceapi,
            nodeapi: nodeapi,
        }
        
    }
    
    
    
    
    
    //get the list of available games for the player to choose from
    //the id of each game, and the number of players in it
    fn get_available_games(&self) -> String{

        println!("The games available are {:?}", self.pods);
        
        let mut toreturn: Vec<(u32, u8)> = Vec::new();

        for (podid, (numberconnected,_,_,_)) in &self.pods{

            if numberconnected < &2 {

                toreturn.push( (*podid, *numberconnected) );
            }
        }

        serde_json::to_string(&toreturn).unwrap()

    }
    
    


    
    //a player who wants to connect to a game
    //join a game and return the address and port and password
    //as a JSON value
    fn connect_to_game(&mut self, gameid: u32) -> String{


        println!("Someones requesting to join game {:?}", gameid);
        
        //if theres a valid external node ip and a valid external nodeport for the pod
        if let Some(nodeexternalip) = self.nodeexternalip.clone(){

            if let Some( ( connectedplayers, _, externalport, password) ) = self.pods.get(&gameid){
                
                if connectedplayers < &2{

                    let addressandport = "ws://".to_string() + &nodeexternalip + ":" + externalport;
                
                    let connectedtogame = ConnectedToGame{
                        addressandport: addressandport,
                        gamepassword: password.clone(),
                    };
                    
                    let toreturn = serde_json::to_string(&connectedtogame).unwrap();
                    
                    return toreturn;
                }
            }
        }
        
        return "no".to_string();        
    }

    
    
    async fn tick(&mut self){
        
        
        self.pods = HashMap::new();
        self.nodeexternalip = None;
        
        
        
        let mut podtointernalip: HashMap<u32, String> = HashMap::new();
        
        
        
        
        
        //get the list of every pod with an ID and IP
        let lp = ListParams::default()
        .timeout(1)
        .labels( &("podtype=gameserver") );
        
        let result = self.podapi.list(&lp).await.unwrap();
        
        for item in result{
            
            let id = item.metadata.labels.clone().unwrap().get("gameserverid").unwrap().clone();
            
            let id = id.parse::<u32>().unwrap();
            
            println!("ids {:?}", id);

            if let Some(ips) = item.status.clone(){
                
                if let Some(ip) = ips.pod_ip{
                    
                    podtointernalip.insert(id, ip);
                }
            }
        }
        

        println!("geetting password and playerss");

        use tokio::time::{timeout, Duration};
        
    
        
        let mut podtopassword: HashMap<u32, String> = HashMap::new();
        let mut podtonumberofconnectedplayers: HashMap<u32, u8> = HashMap::new();

        
        //for every pod with an internal ip
        for (podid, podip) in &podtointernalip{
            
            let address = "http://".to_string() + &podip.clone() + ":8000";
            
            
            if let Ok(timeout) = timeout(Duration::from_secs(1), reqwest::get( &(address.clone() + "/get_players_in_game") )   ).await{
                
                if let Ok(result) = timeout{

                    let body = result.text().await.unwrap();
                
                    if let Ok(playernumb) = body.parse::<u8>(){

                        println!("number of players {:?}", playernumb);
                        
                        podtonumberofconnectedplayers.insert(*podid, playernumb);
                        
                        if playernumb < 2{
                            
                            let password = reqwest::get( &(address + "/get_password") )
                            .await
                            .unwrap()
                            .text()
                            .await
                            .unwrap();

                            println!("got password {:?}", password);
                            
                            podtopassword.insert(*podid, password);
                            
                        }
                    }
                }
                else{
                    println!("request to pod for information timedout");
                }
            }
            else{
                println!("no pod response");
            }
        }
        
        
        println!("geetting external ports");
        
        
        let mut podtoexternalport: HashMap<u32, String> = HashMap::new();
        

        //get every exposer
        let mut exposers: HashSet<u32> = HashSet::new();
        
        
        //get the active node balancers
        //and use it to set the pods by ID to their exposed nodeport
        
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
                                
                                podtoexternalport.insert(exposerid, nodeport.to_string() );
                            }
                        }
                    }
                }
            }
        }
        



        for (id, internalip) in & podtointernalip{

            if let Some(externalport) = podtoexternalport.get(&id){

                if let Some(numberofplayers) = podtonumberofconnectedplayers.get(&id){

                    if let Some(password) = podtopassword.get(&id){

                        println!("ADDING THE VALID GAME TO LIST OF GAMES ID:{:?}", id);

                        self.pods.insert(*id, (*numberofplayers, internalip.clone(), externalport.clone(), password.clone() ) );

                    }
                }
            }
        };







        //make the load balancers that dont exist
        for x in 0..self.podstomake{
            
            if exposers.contains(&x){
                continue;
            }
            else{
                create_external_load_balancer( &self.serviceapi, x).await;
            }
        }
        
        
        
        //for every pod id lacking, create that pod
        for x in 0..self.podstomake{
            
            if podtointernalip.contains_key(&x){
                continue;
            }
            else{
                create_gamepod(&self.podapi, x).await;
            }            
        }
        
        
        
        

        
        //set the external ip
        self.nodeexternalip = None;
        
        //get the address of a random node that i can send back to the client
        //to route the connection through the nodeport to the pod through
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
    
}









use serde::{Serialize, Deserialize};




//the message sent when a client is connected to a game on the server
//and the game is active
#[derive(Serialize, Deserialize)]
pub struct ConnectedToGame{
    
    //the IP and port of the game
    addressandport: String,
    
    //the password of the game
    gamepassword: String,
}


