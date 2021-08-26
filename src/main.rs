#[macro_use] extern crate rocket;


use kube::{
    api::{Api, DeleteParams, ListParams,  PostParams, WatchEvent},
    Client,
};


use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::api::core::v1::Service;
use k8s_openapi::api::core::v1::Node;




use std::net::TcpListener;
use std::net::TcpStream;
use std::process::Command;
use std::{thread, time};
use std::sync::Arc;
use std::sync::Mutex;
//use parking_lot::Mutex;

use std::collections::HashMap;
use std::collections::HashSet;


struct GlobalValues{

}

impl GlobalValues{

    fn get_gamepod_image() -> String{

        "gcr.io/cheaper-324003/github.com/guccialex/ccp-websocket-pod:latest".to_string()
    }


    fn gamepods_needed() -> HashSet<u32>{

        let mut toreturn = HashSet::new();

        for x in 0..20{
            toreturn.insert( x);
        }

        return toreturn;
    }


}




use tokio::task;


#[tokio::main]
async fn main() {
    
    std::env::set_var("RUST_LOG", "info,kube=debug");

    /*
    let mut cfg = Config::from_cluster_env(); // or Config::infer
    cfg.cluster_url = some_uri;
    */


    //holds the most updated version of "Pods"
    let pods = Arc::new(Mutex::new(  PodData::default()  ));


    //tick the pods every 2 seconds
    {
        let pods = pods.clone();

        let (podapi, serviceapi, nodeapi) = get_apis().await;

        let apiwrapper = ApiWrapper{
            pod: podapi,
            service: serviceapi,
            node: nodeapi
        };
    

        task::spawn(async move {

            let tickduration = time::Duration::from_millis(2000);
            let mut interval = tokio::time::interval( tickduration );

            for _i in 0..30000 {
                interval.tick().await;

                let newdata = PodData::update( &apiwrapper ).await;

                *pods.lock().unwrap() = newdata; 

                println!("here");
            }
        });
    }


    loop{

    }



    /*
    rocket::ignite()
    .mount("/api", routes![])
    .launch();
    */

}


pub fn pod_name(id: &u32) -> String{
    return "gamepod".to_string() + &id.to_string() ;
}

//get the name of the nodeport that exposes a service
pub fn nodeport_name(id: &u32) -> String{
    return "nodeport".to_string() + &id.to_string() ;
}

async fn get_apis() -> (Api<Pod>,Api<Service>,Api<Node>){

    let namespace = std::env::var("NAMESPACE").unwrap_or("default".into());

    //connect to the kubernetes pod and service apis
    let client = Client::try_default();
    let client2 = Client::try_default();
    let client3 = Client::try_default();
    
    let (client,client2,client3) = tokio::join!(client,client2,client3);

    let client = client.unwrap();
    let client2 = client2.unwrap();
    let client3 = client3.unwrap();


    let podapi: Api<Pod> = Api::namespaced(client, &namespace);
    let serviceapi: Api<Service> = Api::namespaced(client2, &namespace);
    let nodeapi: Api<Node> = Api::all(client3);

    return (podapi, serviceapi, nodeapi);
}




pub struct ApiWrapper{

    pod: Api<Pod>,

    service: Api<Service>,

    node: Api<Node>,
}

impl ApiWrapper{

    //try to create a new gamepod server
    //with this id
    async fn create_gamepod(podapi: & kube::Api<k8s_openapi::api::core::v1::Pod>, gamepodid: u32){
        
        
        // Create the pod
        let pod: Pod = serde_json::from_value(serde_json::json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": {
                "name": pod_name(&gamepodid),
                
                "labels": {
                    "podtype": "gameserver",
                },
            },
            "spec": {
                "restartPolicy" : "Never",
                "containers": [{
                    "name": "container",
                    "image": GlobalValues::get_gamepod_image(),
                }],
            }
        })).unwrap();
        
        
        let postparams = PostParams::default();
        
        
        podapi.create(&postparams, &pod).await;
    }

    //for this gamepod
    //create a nodeport
    //a nodeport exposes a port and sends all traffic to that port
    async fn create_service_exposing(serviceapi: & Api<Service>, gamepodid: u32 ) {
        
        let serviceid = gamepodid.to_string();

        
        let service: Service = serde_json::from_value(serde_json::json!({
            
            "apiVersion": "v1",
            "kind": "Service",
            "metadata": {

                "name": nodeport_name(&gamepodid),
                
                "labels": {
                    "servicetype": "gamepodexposer",
                },
            },
            "spec": {
                "type": "NodePort",
                
                //select game pods with this name
                "selector": {
                    "name": pod_name(&gamepodid),
                },
                
                "ports": [{
                    "protocol": "TCP",

                    "port": 4000,
                    
                    //should always SEND to port 4000
                    "targetPort": 4000,
                }],
            }
        })).unwrap();
        
        
        let postparams = PostParams::default();
        
        
        serviceapi.create( &postparams, &service).await;
    }




    //get the gamepods that exist
    //unhealthy gamepods are killed by kubernetes automatically (when I configure health checks properly...)
    //and make sure they dont automatically reboot
    async fn get_existing_gamepods(podapi: & Api<Pod>) -> HashMap<u32, Pod>{

        let mut toreturn = HashMap::new();
        
        
        //get the list of every pod with an ID and IP
        let lp = ListParams::default()
        .timeout(1)
        .labels( &("podtype=gameserver") );
        
        let result = podapi.list(&lp).await.unwrap();
        
        for item in result{

            let id = item.metadata.labels.clone().unwrap().get("gameserverid").unwrap().clone();
            
            let id = id.parse::<u32>().unwrap();
            
            toreturn.insert( id, item);

            
            /*
            println!("ids {:?}", id);
            
            if let Some(ips) = item.status.clone(){
                
                if let Some(ip) = ips.pod_ip{
                    
                    self.podstointernalip.insert(id, ip);
                }
            }
            */
        };

        return toreturn;
    }


    async fn get_eternal_ip(nodeapi: &Api<Node> ) -> String{


        //get the address of a random node that i can send back to the client
        //to route the connection through the nodeport to the pod through
        let lp = ListParams::default()
        .timeout(2);
        
        let result = nodeapi.list(&lp).await.unwrap();
        

        //for the node
        for item in &result{
            
            if let Some(status) = &item.status{
                
                if let Some(addresses) = &status.addresses{
                    
                    //for each address
                    for address in addresses{
                        
                        if address.type_ == "ExternalIP"{
                            
                            return address.address.clone();
                        }
                    }
                }
            }
        };

        
        panic!("cant find external ip?");
        
    }


    async fn get_existing_exposers( serviceapi: Api<Service> ) -> HashMap<u32, Service>{

        let mut toreturn = HashMap::new();

        let lp = ListParams::default()
        .timeout(2)
        .labels( "servicetype=gamepodexposer" );
        
        let result = serviceapi.list(&lp).await.unwrap();

        
        //the list of all exposers
        for item in &result{
            
            if let Some(labels) = &item.metadata.labels{
                
                //the serve by the id of the service, that is associated with the pod id
                if let Some(exposerid) = labels.get("serviceid"){
                    
                    let exposerid = exposerid.parse::<u32>().unwrap();

                    toreturn.insert( exposerid, item.clone() );

                    /*
                    exposers.insert( exposerid );
                    
                    if let Some(specs) = &item.spec{
                        
                        if let Some(port) = &specs.ports{
                            
                            if let Some(nodeport) = &port[0].node_port{
                                
                                podtoexternalport.insert(exposerid, nodeport.to_string() );
                            }
                        }
                    }
                    */
                }
            }
        }


        toreturn

    }




}

















//data about all the pods
struct PodData{

    //pods to the number of players allocated
    //internal port
    //external port
    //and password

    //data: HashMap<u32, (u8, String, String, String)>,

    //pods by id to internal ip
    podstointernalip: HashMap<u32, String>,


    externalip: String,

    podstoport: HashMap<u32, String>,

}


impl PodData{

    fn default() -> PodData{


        PodData{

            podstointernalip: HashMap::new(),
            externalip: "none".to_string(),
            podstoport: HashMap::new(),
        }

    }



    //basically tick, but return a pod instead of needing one to mut
    async fn update(apiwrapper: &ApiWrapper) -> PodData{


        let allgamepods = GlobalValues::gamepods_needed();

        let existinggamepods = ApiWrapper::get_existing_gamepods( &apiwrapper.pod ).await;


        for podid in allgamepods{

            if !existinggamepods.contains_key(  &podid ){

                ApiWrapper::create_gamepod( &apiwrapper.pod , podid).await;
            }
        }


        let externalip = ApiWrapper::get_eternal_ip( &apiwrapper.node ).await;

        PodData{

            externalip,

            podstoport: HashMap::new(),

            podstointernalip: HashMap::new(),
        }



        //create needed pods

        //create needed exposers





        //update self

        //update external ip
        //update pods to internal ip
        //update pods to port


    }



}