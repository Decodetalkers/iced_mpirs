//use futures_util::StreamExt;
use once_cell::sync::Lazy;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use zbus::{dbus_proxy, zvariant::OwnedValue, Result};

use zbus::zvariant::OwnedObjectPath;

use std::sync::OnceLock;
#[allow(unused)]
#[derive(Debug, Clone)]
pub struct Metadata {
    mpris_trackid: OwnedObjectPath,
    mpris_arturl: String,
    pub xesam_title: String,
    xesam_album: String,
    xesam_artist: Vec<String>,
}

impl Metadata {
    fn from_hashmap(value: &HashMap<String, OwnedValue>) -> Self {
        let art_url = &value.get("mpris:artUrl");
        let mut mpris_arturl = String::new();
        if let Some(art_url) = art_url {
            mpris_arturl = (*art_url).clone().try_into().unwrap_or_default();
        }

        let trackid = &value["mpris:trackid"];
        let mpris_trackid: OwnedObjectPath = trackid.clone().try_into().unwrap_or_default();

        let title = &value["xesam:title"];
        let xesam_title: String = title.clone().try_into().unwrap_or_default();

        let artist = &value["xesam:artist"];
        let xesam_artist: Vec<String> = artist.clone().try_into().unwrap_or_default();

        let album = &value["xesam:album"];
        let xesam_album: String = album.clone().try_into().unwrap_or_default();

        Self {
            mpris_trackid,
            xesam_title,
            xesam_artist,
            xesam_album,
            mpris_arturl,
        }
    }
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    service_path: String,
    pub can_play: bool,
    pub can_pause: bool,
    pub playback_status: String,
    pub metadata: Metadata,
}

impl ServiceInfo {
    fn new(
        path: &str,
        can_play: bool,
        can_pause: bool,
        playback_status: String,
        value: &HashMap<String, OwnedValue>,
    ) -> Self {
        Self {
            service_path: path.to_owned(),
            can_play,
            can_pause,
            playback_status,
            metadata: Metadata::from_hashmap(value),
        }
    }

    pub async fn pause(&self) -> Result<()> {
        let conn = get_connection().await?;
        let instance = MediaPlayer2DbusProxy::builder(&conn)
            .destination(self.service_path.as_str())?
            .build()
            .await?;
        instance.pause().await?;
        Ok(())
    }

    pub async fn play(&self) -> Result<()> {
        let conn = get_connection().await?;
        let instance = MediaPlayer2DbusProxy::builder(&conn)
            .destination(self.service_path.as_str())?
            .build()
            .await?;
        instance.play().await?;
        Ok(())
    }
}

static SESSION: OnceLock<zbus::Connection> = OnceLock::new();

async fn get_connection() -> zbus::Result<zbus::Connection> {
    if let Some(cnx) = SESSION.get() {
        Ok(cnx.clone())
    } else {
        let cnx = zbus::Connection::session().await?;
        SESSION.set(cnx.clone()).expect("Can't reset a OnceCell");
        Ok(cnx)
    }
}

pub static MPIRS_CONNECTIONS: Lazy<Arc<Mutex<Vec<ServiceInfo>>>> =
    Lazy::new(|| Arc::new(Mutex::new(Vec::new())));

async fn set_mpirs_connection(list: Vec<ServiceInfo>) {
    let mut conns = MPIRS_CONNECTIONS.lock().await;
    *conns = list;
}

#[dbus_proxy(
    default_service = "org.freedesktop.DBus",
    interface = "org.freedesktop.DBus",
    default_path = "/org/freedesktop/DBus"
)]
trait FreedestopDBus {
    #[dbus_proxy(signal)]
    fn name_owner_changed(&self) -> Result<(String, String, String)>;
    fn list_names(&self) -> Result<Vec<String>>;
}

#[dbus_proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait MediaPlayer2Dbus {
    #[dbus_proxy(property)]
    fn can_pause(&self) -> Result<bool>;

    #[dbus_proxy(property)]
    fn playback_status(&self) -> Result<String>;

    #[dbus_proxy(property)]
    fn can_play(&self) -> Result<bool>;

    #[dbus_proxy(property)]
    fn can_go_next(&self) -> Result<bool>;

    #[dbus_proxy(property)]
    fn can_go_previous(&self) -> Result<bool>;

    #[dbus_proxy(property)]
    fn metadata(&self) -> Result<HashMap<String, OwnedValue>>;

    fn pause(&self) -> Result<()>;

    fn play(&self) -> Result<()>;
}

pub async fn init_pris() -> Result<()> {
    let conn = get_connection().await?;
    let freedesktop = FreedestopDBusProxy::new(&conn).await?;
    let names = freedesktop.list_names().await?;
    let names: Vec<String> = names
        .iter()
        .filter(|name| name.starts_with("org.mpris.MediaPlayer2"))
        .cloned()
        .collect();

    let mut serviceinfos = Vec::new();
    for name in names.iter() {
        let instance = MediaPlayer2DbusProxy::builder(&conn)
            .destination(name.as_str())?
            .build()
            .await?;

        let value = instance.metadata().await?;
        let can_pause = instance.can_pause().await?;
        let can_play = instance.can_play().await?;
        let playback_status = instance.playback_status().await?;
        serviceinfos.push(ServiceInfo::new(
            name,
            can_play,
            can_pause,
            playback_status,
            &value,
        ));
    }

    set_mpirs_connection(serviceinfos).await;
    Ok(())
}
