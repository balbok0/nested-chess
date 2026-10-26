use crate::{
    model::Result,
    store::{MATCH_CHANNEL, Store},
};
use sqlx::postgres::PgListener;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::watch;

#[derive(Default)]
pub struct Hub {
    channels: Mutex<HashMap<String, watch::Sender<u32>>>,
}
impl Hub {
    pub fn subscribe(&self, id: &str) -> watch::Receiver<u32> {
        self.channels
            .lock()
            .unwrap()
            .entry(id.into())
            .or_insert_with(|| watch::channel(0).0)
            .subscribe()
    }
    fn publish(&self, id: &str, seq: u32) {
        if let Some(channel) = self.channels.lock().unwrap().get(id) {
            channel.send_if_modified(|current| {
                if seq > *current {
                    *current = seq;
                    true
                } else {
                    false
                }
            });
        }
    }
    async fn catch_up(&self, store: &Store) -> Result<()> {
        let ids = {
            let mut channels = self.channels.lock().unwrap();
            channels.retain(|_, channel| channel.receiver_count() > 0);
            channels.keys().cloned().collect::<Vec<_>>()
        };
        if !ids.is_empty() {
            for (id, seq) in store.versions(&ids).await? {
                self.publish(&id, seq);
            }
        }
        Ok(())
    }
    pub fn start(self: &Arc<Self>, store: Store, mut stopped: watch::Receiver<bool>) {
        let hub = self.clone();
        tokio::spawn(async move {
            let mut listener: Option<PgListener> = None;
            let mut poll = tokio::time::interval(Duration::from_secs(2));
            loop {
                tokio::select! {
                    _=stopped.changed()=>break,
                    _=poll.tick()=>{
                        if let Err(error)=hub.catch_up(&store).await {tracing::warn!(%error,"stream catch-up failed");}
                        if listener.is_none() {
                            match PgListener::connect_with(&store.pool).await {
                                Ok(mut connection)=>match connection.listen(MATCH_CHANNEL).await {
                                    Ok(())=>listener=Some(connection),
                                    Err(error)=>tracing::warn!(%error,"stream listen failed"),
                                },
                                Err(error)=>tracing::warn!(%error,"stream connection failed"),
                            }
                        }
                    },
                    notification=async {match &mut listener {Some(connection)=>connection.recv().await,None=>std::future::pending().await}}=>{
                        match notification {
                            Ok(notification)=>if let Some((id,seq))=notification.payload().rsplit_once(':') {
                                if let Ok(seq)=seq.parse() {hub.publish(id,seq);}
                            },
                            Err(error)=>{tracing::warn!(%error,"stream listener disconnected");listener=None;}
                        }
                    }
                }
            }
        });
    }
}
