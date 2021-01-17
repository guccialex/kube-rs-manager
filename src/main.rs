#![feature(proc_macro_hygiene, decl_macro)]
#[macro_use] extern crate rocket;


use futures::executor::block_on;
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
            
            println!("herereeeee");
            
            //every second
            let sleeptime = time::Duration::from_millis(1000);
            thread::sleep( sleeptime );
            
            //unlock the mutex main while handling this message
            
            let mut main = copiedmutexmain.lock();
            
            main.await.tick().await;
            
        };
        
    });
    
    
    
    //listen for clients who want to be assigned a game on port 8000
    
    let copiedmutexmain = mutexmain.clone();      
    
    
    tokio::spawn(async move {
        
        
        /*
        
        rocket::ignite()
        .manage(copiedmutexmain)
        .mount("/", routes![join_private_game, join_public_game, create_private_game ])
        .launch();
        
        */
        
    });
    
    
    /*
    thread::spawn(move || {
        
        
    });
    */
    
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
                "image": "nginx:1.14.2"
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
    
    
    
    
    //map the pod id to the pods NodePort
    //nodeportofpod: HashMap<u32, u16>,
    
    
    
    podapi: Api<Pod>,
    serviceapi: Api<Service>,
    
}

impl Main{
    
    
    fn new(podapi: Api<Pod>, serviceapi: Api<Service>) -> Main{
        
        Main{
            
            podips: HashMap::new(),
            
            unallocatedpods: Vec::new(),
            openpodandpassword: HashMap::new(),
            
            
            podapi: podapi,
            serviceapi: serviceapi,
        }
    }
    
    //get a nodes ip and a port
    fn get_nodes_ip_and_pods_port(&self, podid: u32) -> String{
        "smmm".to_string()
    }
    
    //set the password of a pod
    fn set_pods_password(&mut self, podid: u32, password: String){
        
        
        
        
    }
    
    
    //set the password of an unallocated pod, and return the id of the pod set
    fn get_unallocated_pod_and_set_password(&mut self, password: String) -> u32{
        
        
        //let podid = self.unallocatedpods.pop().unwrap();
        
        //self.set_pods_password(podid, password);
        
        println!("setting password of unallocated pod");
        
        //return podid;
        
        return 12;
        
    }
    
    
    //a player who wants to connect to a game
    //join a game and return the address and port and password
    //as a JSON value
    fn connect_to_game(&mut self, gametoconnectto: GameToConnectTo) -> String{
        
        
        if let GameToConnectTo::joinprivategame(password) = gametoconnectto{
            
            
            //if a pod with that password exists and is open, return it 
            if let Some(podid) = self.openpodandpassword.remove(&password){
                
                let address = self.get_nodes_ip_and_pods_port(podid);
                
                let connectedtogame = ConnectedToGame{
                    addressandport: address,
                    gamepassword: password,
                };
                
                let toreturn = serde_json::to_string(&connectedtogame).unwrap();
                
                return toreturn;
            }
            //otherwise, set the password of an unallocated pod to that password
            //and return that pod
            else{
                
                self.get_unallocated_pod_and_set_password(password);
                
            }
            
        }
        
        
        
        
        
        
        let connectedtogame = ConnectedToGame{
            addressandport: "google.com".to_string(),
            gamepassword: "fakepassword".to_string(),
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

        
        //for every pod with an ip
        //get its state        
        for (podid, podip) in podswithips{

            //self.podips.insert(podid, podip);
            
            println!("calling the pod with an IP to get its state");

            
            let body = reqwest::get( &(podip + ":4000/get_state") )
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
            
        }
        
        
        
        
        
        
        
        
        //get every load balancer

        let mut load_balancers: HashSet<u32> = HashSet::new();
        
        
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
        

        //make load balancers that dont exist
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





/*


//handle a connection with a client that wants to connect to a game
fn handle_connection(mut newsocket: tungstenite::WebSocket<std::net::TcpStream>, mutexmain: Arc<Mutex<Main>>){
    
    let mut loopnumber = 0;
    
    //loop until i get a message to connect to a certain game
    loop{
        
        //wait 0.5 seconds
        let sleeptime = time::Duration::from_millis(500);
        thread::sleep( sleeptime );
        
        //if this has looped more than 2000 times break
        if loopnumber > 2000{
            break;
        }
        else{
            loopnumber += 1;
        }
        
        //read the first message sent then break
        if let Ok(receivedmessage) = newsocket.read_message(){
            
            //unlock the mutex main while handling this message
            let mut main = mutexmain.lock().unwrap();
            
            let mut connectsucceeded = false;
            
            let message = receivedmessage.to_string();
            
            //if the client wants to connect to a game
            if let Ok(gametoconnectto) = serde_json::from_str::<GameToConnectTo>(&message){
                
                
                //the nodeport of the server for the game the client requested
                //and the password to connect to it
                if let Ok( (nodeport, gamepassword) ) = main.connect_to_game(gametoconnectto){
                    
                    
                    
                    let addressandport = "address".to_string() + &nodeport.to_string();
                    
                    
                    //send the message back to the client about what game its connected to
                    let connectedstructmessage = ConnectedToGame{
                        
                        addressandport: addressandport,
                        
                        gamepassword: gamepassword.to_string(),
                    };
                    
                    let connectedstructstring = serde_json::to_string(&connectedstructmessage).unwrap();
                    
                    
                    let message = Message::text(connectedstructstring);
                    
                    if let Ok(_) = newsocket.write_message(message){    
                    }
                    
                    
                    connectsucceeded = true;
                    
                }
                else{
                    
                    
                }
                
            }
            
            
            //if it succesfully connected and sent that message to the client
            if connectsucceeded{
                break;
            }
        }
        
        
    };
    
    
}





use std::collections::HashMap;

use std::collections::HashSet;


struct Main{
    
    //the mapping of each pod to its IP
    podips: HashMap< u32, String >,
    
    
    //the pods in the 4 different states
    //if its not responding to pings yet and isnt operating yet
    //if it hasnt had its password set yet
    //get if it has a password set
    //get if it has both players registered
    
    unallocatedpods: HashSet<u32>,
    
    //the map of each password to the gamepods id
    openpodandpassword: HashMap<u32, String>,
    
    
    
    //map the pod id to the pods NodePort
    nodeportofpod: HashMap<u32, u16>,
    
    
    
    
    
    podapi: Api<Pod>,
    serviceapi: Api<Service>,
    
}

impl Main{
    
    fn new(podapi: Api<Pod>, serviceapi: Api<Service>) -> Main{
        
        Main{
            
            podips: HashMap::new(),
            
            
            unallocatedpods: HashSet::new(),
            openpodandpassword: HashMap::new(),
            
            
            
            nodeportofpod: HashMap::new(),
            
            
            podapi: podapi,
            serviceapi: serviceapi,
        }
    }
    
    //given a podid get its password
    fn get_password_of_pod(&self, podid: u32) -> String{
        
        for (curpodid, curpodpassword) in self.openpodandpassword{
            
            if curpodid == podid{
                
                return curpodpassword;
            }
            
        }
        
        panic!("pod with that ID doesnt exist");
        
    }
    
    
    //return the nodeport of the game the client should connect to
    //and the password
    //if its a valid thing connection request
    fn connect_to_game(&mut self, gametoconnectto: GameToConnectTo) -> Result< (u16, String), () >{
        
        
        if let GameToConnectTo::createprivategame = gametoconnectto{
            
            
            //if i dont have any unallocated pods, dont panic, just dont return anything
            let podid = self.unallocatedgames.pop().ok_or(())?;
            
            self.opengames.insert(podid);
            
            
            //return the NodePort of the gamepod and the password
            let password = self.get_password_of_pod(podid);
            
            let nodeport = *self.nodeportofpod.get(&podid).unwrap();
            
            return Ok(  (nodeport, password) );
            
            
        }
        
        else if let GameToConnectTo::joinprivategame(password) = gametoconnectto{
            
            //get the game with that password
            let podid = self.gamepassword.get(&password).ok_or(())?;
            
            self.opengames.remove(podid);
            
            self.runninggames.push(*podid);
            
            
            let nodeport = *self.nodeportofpod.get(&podid).unwrap();
            
            
            return Ok( (nodeport, password) );
            
            
        }
        else if let GameToConnectTo::joinpublicgame = gametoconnectto{
            
            //if theres an open public game 
            if let Some(podid) = self.openpublicgame{
                
                self.openpublicgame = None;
                
                self.opengames.remove(&podid);
                
                self.runninggames.push(podid);
                
                let nodeport = *self.nodeportofpod.get(&podid).unwrap();
                
                let password = self.get_password_of_pod(podid);
                
                
                return Ok( (nodeport, password) );
                
            }
            //otherwise create a new game
            else{
                
                //if i dont have any unallocated pods, dont panic, just dont return anything
                let podid = self.unallocatedgames.pop().ok_or(())?;
                
                self.opengames.insert(podid);
                self.openpublicgame = Some(podid);
                
                //return the NodePort of the gamepod and the password
                let password = self.get_password_of_pod(podid);
                
                let nodeport = *self.nodeportofpod.get(&podid).unwrap();
                
                return Ok(  (nodeport, password) );
            }
            
            
        }
        
        
        Err( () ) 
        
        
        
    }
    
    
    //update the list of pods I have and their IPs
    fn update_pod_ips(&mut self){
        
        //the list of pods
        //ID to its IP
        let mut podips: HashMap<u32, String> = HashMap::new();
        
        
        
        //get the list of every gameserver podtype pod in the cluster
        let lp = ListParams::default()
        .timeout(3)
        .labels("servicetype=gamepodexposer");
        
        
        let result = self.podapi.list(&lp);
        let result = block_on(result).unwrap();
        
        for item in result{
            
            let podip = item.status.unwrap().pod_ips.unwrap()[0].ip.clone().unwrap();            
            
            //if it is a pod that is not in the list of pods, set up a websocket connection with it
            //and put it in the list of unallocated games
            let podid = item.metadata.labels.unwrap().get("gamepodid").unwrap();
            
            let podidnumber = podid.parse::<u32>().unwrap();
            
            podips.insert( podidnumber, podip  );
        }
        
        
        self.podips = podips;
        
    }
    
    
    
    
    
    //there can be multiple matchmakers
    fn tick(&mut self){
        
        
        //update the pod ids that this struct knows about
        self.update_pod_ips();
        
        //create the pods that are needed
        self.create_needed_pods();
        
        //update the state of the pods
        self.update_pod_states();
        
        
        
        
        /*
        
        Active Pods: PodID, IP
        
        
        
        
        
        */
        
        //every tick update the state of this struct
        
        //to reflect the current state of the pods
        
        
        //I need:
        //pods with unset password
        //pods with set password awaiting the other player to register
        
        
        
        //get every gamepod that has an IP
        
        //get every gamepod id and port
        
        //update the 
        
        
        
        
        
        
        
        
        /*
        //try to connect to that pod on port 8880 and sent a message
        let mut gamepodip: String = "".to_string();
        
        //open a websocket connection with any server 
        let webaddress = gamepodip;
        let gameport = 8880.to_string();
        
        let addressandport = webaddress + ":" + &gameport;
        
        use tungstenite::client::connect;
        
        let (mut socket, response) = connect( addressandport ).expect("Can't connect");
        
        
        socket.write_message(Message::Text("Hello WebSocket".into())).unwrap();
        
        loop {
            let msg = socket.read_message().expect("Error reading message");
            println!("Received: {}", msg);
        }
        */
        
        
        //get every pod by ID
        let mut podids: HashSet<u32> = HashSet::new();
        
        //the IP of every pod
        let mut podidtoip: HashMap<u32, u16> = HashMap::new();
        
        
        
        
        
        //get the nodeport of every pod
        let mut podidtonodeport: HashMap<u32, u16> = HashMap::new();
        
        
        
        //get the list of every nodeport service
        {
            
            let lp = ListParams::default()
            .timeout(60)
            .labels( "servicename=gamepodexposer" );
            
            let result = self.serviceapi.list(&lp);
            let result = block_on(result).unwrap();
            
            
            
            for item in result{
                
                if let metadata = item.metadata{
                    
                    if let Some(label) = metadata.label{
                        
                        
                        
                        
                    }
                    
                }
                
                
                if let Some(spec) = item.spec{
                    
                    if let Some(ports) = spec.ports{
                        
                        let firstport = &ports[0];
                        
                        if let Some(nodeport) = firstport.node_port{
                            
                            println!("the nodeport value is {:?}", nodeport);
                            
                            podidtonodeport.insert(  nodeport);
                        }           
                    }
                }
            }
            
            let sleeptime = time::Duration::from_millis(1000);
            thread::sleep( sleeptime );
            
            
        }
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
    }
    
    
}


*/






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