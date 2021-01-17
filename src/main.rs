#![feature(proc_macro_hygiene, decl_macro)]
#[macro_use] extern crate rocket;


use kube::{
    api::{Api, DeleteParams, ListParams, Meta, PostParams, WatchEvent},
    Client,
};


use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::api::core::v1::Service;
use k8s_openapi::api::batch::v1::Job;


use tungstenite::{Message};
use tungstenite::handshake::server::{Request, Response};
use tungstenite::accept_hdr;


use std::net::TcpListener;
use std::net::TcpStream;
use std::process::Command;
use std::{thread, time};
use std::sync::Arc;
use std::sync::Mutex;






use rocket::State;


#[tokio::main]
async fn main() {
    
    std::env::set_var("RUST_LOG", "info,kube=debug");
    
    
    let namespace = std::env::var("NAMESPACE").unwrap_or("default".into());
    
    //connect to the kubernetes pod and service apis
    let client = Client::try_default();
    let client2 = Client::try_default();
    
    
    let client = client.await.unwrap();//block_on(client).unwrap();
    let podapi: Api<Pod> = Api::namespaced(client, &namespace);
    
    let client2 = client2.await.unwrap();
    let serviceapi: Api<Service> = Api::namespaced(client2, &namespace);
    
    

    
    
    let mutexmain = Arc::new(tokio::sync::Mutex::new(Main::new(podapi, serviceapi)));
    
    
    //a new thread that ticks the main every second
    let copiedmutexmain = mutexmain.clone();
    
    
    
    tokio::spawn(async move {
        
        //this loops to make sure the mutex main does what the functions says it does when the functions in the
        //websocket loop call it
        
        loop {

            println!("ticking");
            
            //every second
            let sleeptime = time::Duration::from_millis(1000);
            thread::sleep( sleeptime );
            
            //unlock the mutex main while handling this message
            
            let mut main = copiedmutexmain.lock().await;
            
            main.tick().await;
            
        };
        
    });
    
    
    
    //listen for clients who want to be assigned a game on port 8000
    
    let copiedmutexmain = mutexmain.clone();      
    
    
    thread::spawn(move || {
        
        rocket::ignite()
        .manage(copiedmutexmain)
        .mount("/", routes![join_private_game, join_public_game, create_private_game ])
        .launch();
        
    });
    
    
    loop{
    }
    
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
                "image": "gcr.io/level-unfolding-299521/github.com/guccialex/ccp-websocket-server@sha256:d12306bab913b8c1af42db46f4c55c09188eda7c121e716921c97814a658e1ee"
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
            
            "ports": [{
                "protocol": "TCP",
                "port": 80,
                "targetPort": 80,
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




use std::collections::{HashMap, HashSet};


struct Main{
    
    
    //the mapping of each pod to its IP
    podips: HashMap< u32, String >,
    
    //the pods that dont have a password set yet
    unallocatedpods: Vec<u32>,
    
    //the map of each password to the gamepods id
    openpodandpassword: HashMap<String, u32>,


    nodeidtoaddressandport: HashMap<u32, String>,
    
    
    
    
    
    podapi: Api<Pod>,
    serviceapi: Api<Service>,
    
}

impl Main{
    
    
    fn new(podapi: Api<Pod>, serviceapi: Api<Service>) -> Main{
        
        Main{
            
            podips: HashMap::new(),
            
            unallocatedpods: Vec::new(),
            openpodandpassword: HashMap::new(),

            nodeidtoaddressandport: HashMap::new(),
            
            
            podapi: podapi,
            serviceapi: serviceapi,
        }
    }
    
    
    
    //set the password of an unallocated pod, and return the id of the pod set
    fn get_unallocated_pod_and_set_password(&mut self) -> (u32, String){
        
        
        let podid = self.unallocatedpods.pop().unwrap();
        
        let podip = self.podips.remove(&podid).unwrap();
                  

        use rand::{distributions::Alphanumeric, Rng};

        let passwordtoset: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(7)
        .map(char::from)
        .collect();
        

        let address = "http://".to_string() + &podip.clone() + ":4000";


        let resp = reqwest::blocking::get(  &(address.to_string() + "/set_password/"+ &passwordtoset)  );

        
        println!("setting password of unallocated pod");
        

        //return podid;
        return (podid, passwordtoset);
        
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

                let (podid, password) = self.get_unallocated_pod_and_set_password();

                thepodid = podid;
                thepassword = password;

            }


        }
        else if let GameToConnectTo::createprivategame = gametoconnectto{
                
            let (podid, password) = self.get_unallocated_pod_and_set_password();

            thepodid = podid;
            thepassword = password;

        }
        else{
            panic!("hmm");
        }
        

        //maybe before returning, send a message to the pod, that a player has just been allocated to it


        let address = self.nodeidtoaddressandport.get(&thepodid).unwrap().to_string();

        let connectedtogame = ConnectedToGame{
            addressandport: address,
            gamepassword: thepassword,
        };
        
        let toreturn = serde_json::to_string(&connectedtogame).unwrap();
        
        return toreturn;

        
        
    }
    
    
    
    async fn tick(&mut self){
        
        
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
        
        
        
        //clear the list of unallocated pods
        self.unallocatedpods = Vec::new();
        //and the list of open pods with a password set
        self.openpodandpassword = HashMap::new();
        
        self.podips = HashMap::new();

        println!("podips {:?}", podswithips);
        
        
        //for every pod with an ip
        //get its state        
        for (podid, podip) in podswithips{
            
            //self.podips.insert(podid, podip);

            let address = "http://".to_string() + &podip.clone() + ":4000";

            println!("calling the pod with an IP to get its state {:?}", address);
            
            if let Ok(result) = reqwest::get( &(address.clone() + "/get_state") ).await{

                let body = result.text().await.unwrap();


                if let Ok(statusnumber) = body.parse::<u32>(){
                
                    //if the password isnt set
                    if statusnumber == 1{
                        
                        self.unallocatedpods.push(podid);
                        self.podips.insert(podid, podip);
                        
                    }
                    //if the password is set, and there are players left to be assigned
                    else if statusnumber == 2{
                        
                        let password = reqwest::get( &(address + "/get_password") )
                        .await
                        .unwrap()
                        .text()
                        .await
                        .unwrap();
                        
                        self.openpodandpassword.insert(password, podid);
                        self.podips.insert(podid, podip);
                        
                    }
                    else if statusnumber == 3{
                    }
                    else{
                    }
                }

                println!("status body result {:?}", body);
            }    
        }
        
        
        
        
        
        
        //get every load balancer
        
        let mut load_balancers: HashSet<u32> = HashSet::new();
        
        //get the active node balancers
        //TODO: and use it to set the pods by ID to their address and nodeport
        
        {
            let lp = ListParams::default()
            .timeout(2)
            .labels( "servicetype=gamepodexposer" );
            
            let result = self.serviceapi.list(&lp).await.unwrap();
            
            for item in result{
                
                if let Some(labels) = item.metadata.labels{
                    
                    if let Some(exposerid) = labels.get("serviceid"){
                        
                        println!("serviceid {:?}", exposerid);
                        
                        load_balancers.insert( exposerid.parse::<u32>().unwrap() );
                        
                    }
                }
            }
        }
        
        
        //make the load balancers that dont exist
        for x in 0..5{
            
            if load_balancers.contains(&x){
                
                continue;
            }
            else{
                create_external_load_balancer( &self.serviceapi, x).await;
            }
        }
        
        
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