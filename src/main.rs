
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::batch::v1::Job;

use futures::executor::block_on;


use kube::{
    api::{Api, DeleteParams, ListParams, Meta, PostParams, WatchEvent},
    Client,
};

use k8s_openapi::api::core::v1::Pod;


use std::net::TcpListener;
use std::net::TcpStream;
use std::{thread, time};

use tungstenite::{Message};
use std::process::Command;

use std::sync::Arc;
use  std::sync::Mutex;


use tungstenite::handshake::server::{Request, Response};
use tungstenite::accept_hdr;


#[tokio::main]
async fn main() {
    
    std::env::set_var("RUST_LOG", "info,kube=debug");
    
    
    let client = Client::try_default();
    let client = block_on(client).unwrap();
    
    let namespace = std::env::var("NAMESPACE").unwrap_or("default".into());
    
    let podapi: Api<Pod> = Api::namespaced(client, &namespace);
    
    
    //on startup, delete every old game pod
    let lp = ListParams::default()
    .timeout(60)
    .labels("podtype=gameserver");
    
    let result = podapi.list(&lp);
    let result = block_on(result).unwrap();
    
    for item in result{
        
        if let Some(status) = item.status{
            
            if let Some(podips) = status.pod_ips{
                
                let podip = podips[0].ip.clone().unwrap();
                
                println!("the pods IP {:?}", podip  );
                println!("");
                println!("the pods unique ID {:?}", item.metadata.labels.unwrap().get("gamepodid").unwrap());
                
                let podname = item.metadata.name.unwrap();

                // Delete the pod
                let deleteparams = DeleteParams::default();
                //delete the pod
                let deletedata = podapi.delete(&podname, &deleteparams);
                let deletedata = block_on(deletedata);
            
                println!("getting rid of  pod");
                
            }
        }
    }
    



    
    println!("im here");
    
    
    
    let mut totalgamepods = 0;
    
    
    //create 10 nginx pods to spin ups
    
    for podnumber in 0..10{
        
        
        totalgamepods += 1;
        
        
        let podname = "gamepod".to_string() + &totalgamepods.to_string();
        
        
        // Create the pod
        
        let pod: Pod = serde_json::from_value(serde_json::json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": {
                "name": podname,
                
                "labels": {
                    "podtype": "gameserver",
                    
                    "gamepodid": totalgamepods.to_string(),
                },
            },
            "spec": {
                "containers": [{
                    "name": "gamepod",
                    "image": "nginx:1.14.2"
                }],
            }
        })).unwrap();
        
        
        
        let postparams = PostParams::default();
        
        
        let pod = podapi.create(&postparams, &pod);
        let pod = block_on(pod).unwrap();
        
        
        
    }
    
    
    
    
    
    
    
    
    
    
    
    /*
    
    
    
    
    //open a websocket connection with any server 
    let webaddress = "0.0.0.0".to_string();
    let gameport = 3050.to_string();
    
    
    let listener = TcpListener::bind(webaddress + ":" + &gameport).unwrap();
    
    
    
    
    
    let mutexmain = Arc::new(Mutex::new(Main::new(podapi)));
    
    
    
    
    let copiedmutexmain = mutexmain.clone();
    
    //a new thread that ticks the main every second
    thread::spawn(move || {
        
        for x in 0..100000{
            
            //every second
            let sleeptime = time::Duration::from_millis(1000);
            thread::sleep( sleeptime );
            
            //unlock the mutex main while handling this message
            let mut main = copiedmutexmain.lock().unwrap();
            
            main.tick();
        };
        
        panic!("I should restart the server now");
        
    });    
    
    
    
    
    
    //for each websocket stream this server gets
    for stream in listener.incoming() {
        
        //accept a new websocket 10 times every second
        let sleeptime = time::Duration::from_millis(100);
        thread::sleep( sleeptime );
        
        
        let copiedmutexmain = mutexmain.clone();
        
        
        //spawn a new thread for the connection
        thread::spawn(move || {
            
            let stream = stream.unwrap();
            
            stream.set_nonblocking(true);
            
            let callback = |req: &Request, mut response: Response| {
                Ok(response)
            };
            
            //panic and exit the thread if its not a websocket connection
            let websocket = accept_hdr(stream, callback).unwrap();
            
            
            handle_connection( websocket, copiedmutexmain );
            
        });    
        
        
    }
    
    */
    
    
    
    
    
    
    
    
    
}



fn handle_connection(mut newsocket: tungstenite::WebSocket<std::net::TcpStream>, mutexmain: Arc<Mutex<Main>>){
    
    
    
    
    let mut loopnumber = 0;
    
    //loop until i get a message to connect to a certain game
    loop{
        
        
        //wait 1 second
        let sleeptime = time::Duration::from_millis(1000);
        thread::sleep( sleeptime );
        
        
        //if this has looped more than 2000 times break
        if loopnumber > 2000{
            break;
        }
        else{
            loopnumber += 1;
        }
        
        
        
        if let Ok(receivedmessage) = newsocket.read_message(){
            
            //unlock the mutex main while handling this message
            let mut main = mutexmain.lock().unwrap();
            
            let mut connectsucceeded = false;
            
            let message = receivedmessage.to_string();
            
            println!("i received this message through the websocket {}", message);
            
            
            
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
    
    
    //the id of the pod, increases by 1 every time a new pod is created
    curpodid: u32,
    
    
    //the websocket of every game server
    //and its ID (the 0 to 200 value after the name)
    games: HashMap<u32, tungstenite::WebSocket<std::net::TcpStream>>,
    
    
    //the list of unallocated games
    unallocatedgames: HashSet<u32>,
    
    //the games that are open and looking for another player
    opengames: HashMap<MatchPassword, u32>,
    
    //the closed to accepting player connection games that have 2 players in a game on them
    closedgames: HashSet<u32>,
    
    
    //the public password is the same for every public game
    publicpassword: String,
    
    podapi: Api<Pod>,
    
}

impl Main{
    
    fn new(podapi: Api<Pod>) -> Main{
        
        Main{
            
            curpodid: 0,
            
            unallocatedgames: HashSet::new(),
            
            games: HashMap::new(),
            opengames: HashMap::new(),
            closedgames: HashSet::new(),
            
            
            publicpassword: "password".to_string(),
            
            podapi: podapi,
        }
    }
    
    
    
    //return the port to the game given the password to the match
    fn connect_to_game(&mut self, matchpassword: MatchPassword) -> (u16, String){



        //the pods expose port 4000 for their connection with the matchmaking server
        //and expose port 3000 for connecting to the external players
        
        
        
        //see if this game is in hte list of open games
        //if it is, return the port and the password
        
        if let Some(gameid)  = self.opengames.get(&matchpassword){
            
            
            return ( 3000, self.publicpassword.clone()  );
        }
        
        
        (3000, self.publicpassword.clone() )
        
        
        
        
        //go through the list of open games
        //get a game with that password
        
        //if that 
        
        
        
        //list of unallocated games
        
        
    }
    
    
    //tick only once every 30 seconds. this is important
    //because this logic relies on not creating more games when last tick i already sent games to be created
    //that havent been spun up yet
    fn tick(&mut self){
        
        
        //get the list of every gameserver podtype pod in the cluster
        
        let lp = ListParams::default()
        .timeout(3)
        .labels("podtype=gameserver");
        
        let result = self.podapi.list(&lp);
        let result = block_on(result).unwrap();
        
        for item in result{
            
            let podip = item.status.unwrap().pod_ips.unwrap()[0].ip.clone().unwrap();
            
            //println!("the result {:?}", item);
            //println!("{:?}", item.metadata);
            println!("the pods IP {:?}", podip  );
            println!("");
            println!("");
            
            
            
            
            //if it is a pod that is not in the list of pods, set up a websocket connection with it
            //and put it in the list of unallocated games
            let podid = item.metadata.labels.unwrap().get("gamepodid").unwrap();
            
            
            //if this pod isnt already in my list of pods
            //or a pod that has been removed
            //WHEN a websocket connection can be established with this pod
            //then  add it to the "games" list, and the list of unallocated pods
            
            
            
            
            
        }
        
        
        
        
        //remove unhealthy pods
        {
            let mut unhealthypods = Vec::new();
            
            
            //for every pod send a message requesting a health check
            for (podid, websocket) in self.games.iter_mut(){
                
                let message = Message::text("are you healthy?");
                
                
                if let Ok(_) = websocket.write_message(message){
                    
                }
                else{
                    unhealthypods.push(podid.clone());
                }
            }

            //wait some time or something for it to get to checking its incoming websocket messages
            //and then responding
            let sleeptime = time::Duration::from_millis(300);
            thread::sleep( sleeptime );
            
            
            //for every pod, get if its health check passed
            for (podid, websocket) in self.games.iter_mut(){
                
                
                if let Ok(receivedmessage) = websocket.read_message(){
                    
                    if receivedmessage == Message::text("i'm healthy!") {
                        
                        //go to the next iteration without marking this pod unhealthy
                        continue;
                    }
                }
                
                unhealthypods.push(podid.clone());
            }
            
            
            //if the game is not healthy
            //remove it from the list of open games if it was in it
            //and tell the kubernetes cluster to delete it
            for podid in unhealthypods{
                
                let podname = "gamepod".to_string() + &podid.to_string();
                
                // Delete the pod
                let deleteparams = DeleteParams::default();
                
                //delete the pod
                let deletedata = self.podapi.delete(&podname, &deleteparams);
                
                let deletedata = block_on(deletedata);
                
                
                
                
                self.games.remove(&podid);
                self.unallocatedgames.remove(&podid);
                //TODO, REMOVE IT FROM TEH LIST OF OPEN GAMES
                //self.opengames.remove(&podid);
                self.closedgames.remove(&podid);
                
                
                println!("getting rid of  pod");
                
            }
            
        }
        
        
        
        
        
        
        
        let mut additionalpodsneeded = 5 - (self.unallocatedgames.len() as i32);
        
        if additionalpodsneeded < 0{
            additionalpodsneeded = 0;
        }
        
        
        //tell the server to spin up that difference between how many pods I have and want
        //maybe i should only spin up up to 2 pods per tick or some limiting number
        for x in 0..additionalpodsneeded{
            
            let podname = "gamepod".to_string() + &self.curpodid.to_string();
            
            
            // Create the pod
            let pod: Pod = serde_json::from_value(serde_json::json!({
                "apiVersion": "v1",
                "kind": "Pod",
                "metadata": {
                    "name": podname,
                    
                    "labels": {
                        "podtype": "gameserver",
                        "gamepodid": self.curpodid.to_string(),
                    },
                },
                "spec": {
                    "containers": [{
                        "image": "nginx:1.14.2"
                    }],
                }
            })).unwrap();
            
            
            let postparams = PostParams::default();    
            
            let pod = self.podapi.create(&postparams, &pod);
            let pod = block_on(pod).unwrap();
            
            self.curpodid += 1;
        }
        
        
    }
    
    
}




//the id of a match
#[derive(PartialEq, Eq, Hash)]
enum MatchPassword{
    
    //a private game with an associated password
    Private(String),
    
    //a public game
    Public,    
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
    
    //the port of the game
    gameport: u16,
    
    //the password of the game
    gamepassword: String,
}