extern crate serde_json;

use rumqtt::{MqttClient, MqttOptions, QoS, SecurityOptions};
use snips_nlu_lib::SnipsNluEngine;
use snips_nlu_ontology::{IntentParserResult};
use std::str;
use std::process;
use crate::schema::config;
use crate::schema::hermes;

pub fn start(config: &config::Config, engine: &SnipsNluEngine) {

    let mqtt_options = MqttOptions::new("snips-nlu-rebirth", config.mqtt.host.clone(), config.mqtt.port.clone())
        .set_keep_alive(30)
        .set_security_opts(SecurityOptions::UsernamePassword(config.mqtt.username.clone(), config.mqtt.password.clone()));

    let _conn = match MqttClient::start(mqtt_options) {
        Ok(c) => {
            let (mut mqtt_client, notifications) = c;

            mqtt_client.subscribe("hermes/nlu/#", QoS::AtLeastOnce).unwrap();

            for notification in notifications {
                match notification {
                    rumqtt::client::Notification::Publish(packet) => {
                        let query = str::from_utf8(&packet.payload).unwrap();

                        if &packet.topic_name == "hermes/nlu/query" {
                            hermes_nlu_query(&mut mqtt_client, &engine, &query);
                        } else if &packet.topic_name == "hermes/nlu/exit" {
                            let ret_code: i32 = match str::FromStr::from_str(query) {
                                Ok(rc) => { rc },
                                Err(_) => {
                                    println!("\nBad exit return code, using 0x01");
                                    1
                                }
                            };

                            process::exit(ret_code);
                        }
                    },
                    _ => {}
                }
            }
        }
        Err(e) => {
            match e {
                rumqtt::error::ConnectError::MqttConnectionRefused(_mqtt_error) => {

                },
                rumqtt::error::ConnectError::Io(_io_error) => {

                },
                _ => {}
            }

        }
    };
}

pub fn hermes_error_nlu(mqtt_client: &mut MqttClient, parsed_query: Option<hermes::NluQuery>, error_message: &str) {
    let nlu_error: hermes::NluError = hermes::NluError {
        sessionId: match parsed_query {
            Some(pq) => { pq.sessionId },
            None => { None }
        },
        error: String::from(error_message),
        context: None
    };

    let result_json = serde_json::to_string(&nlu_error).unwrap();
    mqtt_client.publish("hermes/error/nlu", QoS::AtLeastOnce, false, result_json).unwrap();
}

pub fn hermes_nlu_intent_not_recognized(mqtt_client: &mut MqttClient, parsed_query: hermes::NluQuery) {
    let nlu_intent_not_recognized: hermes::NluIntentNotRecognized = hermes::NluIntentNotRecognized {
        input: parsed_query.input,
        id: parsed_query.id,
        sessionId: parsed_query.sessionId
    };
    let result_json = serde_json::to_string(&nlu_intent_not_recognized).unwrap();
    mqtt_client.publish("hermes/nlu/intentNotRecognized", QoS::AtLeastOnce, false, result_json).unwrap();
}

pub fn hermes_nlu_intent_parsed(mqtt_client: &mut MqttClient, parsed_query: hermes::NluQuery, parsed_result: IntentParserResult) {
    let nlu_intent_parsed: hermes::NluIntentParsed = hermes::NluIntentParsed {
        input: parsed_query.input,
        id: parsed_query.id,
        sessionId: parsed_query.sessionId,
        intent: parsed_result.intent,
        slots: parsed_result.slots
    };
    let result_json = serde_json::to_string(&nlu_intent_parsed).unwrap();
    mqtt_client.publish("hermes/nlu/intentParsed", QoS::AtLeastOnce, false, result_json).unwrap();
}

pub fn hermes_nlu_query(mqtt_client: &mut MqttClient, engine: &SnipsNluEngine, query: &str) {
    let parsed_query: hermes::NluQuery = match serde_json::from_str(&query) {
        Ok(pq) => { pq }
        Err(e) => {
            hermes_error_nlu(mqtt_client, None, &e.to_string());
            return;
        }
    };

    if parsed_query.input.is_none() {
        hermes_error_nlu(mqtt_client, Some(parsed_query), "No input field");
        return;
    }

    if parsed_query.sessionId.is_none() {
        hermes_error_nlu(mqtt_client, Some(parsed_query), "No sessionId field");
        return;
    }

    let intents_alternatives = 0;
    let slots_alternatives = 0;
    let input = parsed_query.input.as_ref().unwrap();
    let parsed_result = engine.parse_with_alternatives(&*input, None, None, intents_alternatives, slots_alternatives).unwrap();

    if parsed_result.intent.intent_name.is_none() {
        //IntentParserResult { input: "l", intent: IntentClassifierResult { intent_name: None, confidence_score: 0.54227686 }, slots: [], alternatives: [] }
        hermes_nlu_intent_not_recognized(mqtt_client, parsed_query);
        return;
    }

    //IntentParserResult { input: "light in the garage", intent: IntentClassifierResult { intent_name: Some("turnLightOn"), confidence_score: 0.3685922 }, slots: [Slot { raw_value: "garage", value: Custom(StringValue { value: "garage" }), alternatives: [], range: 13..19, entity: "room", slot_name: "room", confidence_score: None }], alternatives: [] }
    hermes_nlu_intent_parsed(mqtt_client, parsed_query, parsed_result);
    return;
}
