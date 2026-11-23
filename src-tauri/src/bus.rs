//! The session bus and the plumbing every KDE Connect call goes through.
//!
//! kdeconnectd exposes Qt objects: property bags, variants, structs. Nothing
//! here models them as Rust types — a reply becomes JSON once (`to_json`) and
//! `kdc.rs` picks the fields it names. One converter instead of a proxy per
//! plugin, and an interface that gains a property never breaks a build.

use std::collections::HashMap;
use std::time::Duration;

use serde_json::{Map, Value as Json};
use zbus::zvariant::{DynamicType, OwnedValue, Structure, Value};
use zbus::Connection;

pub const SERVICE: &str = "org.kde.kdeconnect";
pub const ROOT: &str = "/modules/kdeconnect";
const PROPS: &str = "org.freedesktop.DBus.Properties";

/// A D-Bus value as plain JSON: variants unwrapped, structs as arrays.
pub fn to_json(v: &Value<'_>) -> Json {
    match v {
        Value::U8(x) => Json::from(*x),
        Value::Bool(x) => Json::from(*x),
        Value::I16(x) => Json::from(*x),
        Value::U16(x) => Json::from(*x),
        Value::I32(x) => Json::from(*x),
        Value::U32(x) => Json::from(*x),
        Value::I64(x) => Json::from(*x),
        Value::U64(x) => Json::from(*x),
        Value::F64(x) => Json::from(*x),
        Value::Str(s) => Json::from(s.as_str()),
        Value::Signature(s) => Json::from(s.to_string()),
        Value::ObjectPath(p) => Json::from(p.as_str()),
        Value::Value(inner) => to_json(inner),
        Value::Array(a) => Json::Array(a.iter().map(to_json).collect()),
        Value::Structure(s) => Json::Array(s.fields().iter().map(to_json).collect()),
        Value::Dict(d) => {
            let mut m = Map::new();
            for (k, val) in d.iter() {
                let key = match k {
                    Value::Str(s) => s.as_str().to_string(),
                    other => to_json(other).to_string(),
                };
                m.insert(key, to_json(val));
            }
            Json::Object(m)
        }
        // file descriptors: nothing kdeconnectd hands a UI
        _ => Json::Null,
    }
}

/// What went wrong, in the daemon's own words when it gave any.
pub fn explain(e: zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(name, detail, _) => match detail {
            Some(d) if !d.is_empty() => d,
            _ => name.to_string(),
        },
        other => other.to_string(),
    }
}

#[derive(Clone)]
pub struct Bus {
    conn: Connection,
    /// `EWE_MOBILE_NO_DAEMON=1`: never start kdeconnectd, not even by
    /// talking to its name (the bus activates it on ANY method call). A test
    /// run on a private bus would otherwise become a new device on the
    /// network — the same guard as the ewe.phone plugin's bridge.
    no_daemon: bool,
}

/// How long the daemon gets to answer. It answers from memory in a few
/// milliseconds; a call that waits longer than this is a hung daemon, and a
/// command that never returns would leave its page waiting for good.
const ANSWER: Duration = Duration::from_secs(20);
/// … except the ones that wait on the phone (mounting its storage).
pub const PATIENT: Duration = Duration::from_secs(45);

async fn within<T>(
    limit: Duration,
    call: impl std::future::Future<Output = zbus::Result<T>>,
) -> Result<T, String> {
    match tokio::time::timeout(limit, call).await {
        Ok(r) => r.map_err(explain),
        Err(_) => Err("KDE Connect didn’t answer.".into()),
    }
}

/// `EWE_MOBILE_NO_DAEMON` set to anything but nothing or `0`.
fn no_daemon_asked(value: Option<std::ffi::OsString>) -> bool {
    value.is_some_and(|v| !v.is_empty() && v != "0")
}

impl Bus {
    pub async fn connect() -> Result<Self, String> {
        let conn = Connection::session()
            .await
            .map_err(|e| format!("no session bus: {e}"))?;
        Ok(Self {
            conn,
            no_daemon: no_daemon_asked(std::env::var_os("EWE_MOBILE_NO_DAEMON")),
        })
    }

    /// A handle that only ever looks: `--status` must not start the daemon.
    pub fn passive(mut self) -> Self {
        self.no_daemon = true;
        self
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// The daemon's own address on the bus (`:1.42`), None while it is not
    /// running. Asking never starts it.
    pub async fn owner(&self) -> Option<String> {
        let dbus = zbus::fdo::DBusProxy::new(&self.conn).await.ok()?;
        let name = SERVICE.try_into().ok()?;
        dbus.get_name_owner(name)
            .await
            .ok()
            .map(|o| o.as_str().to_string())
    }

    pub async fn daemon_running(&self) -> bool {
        self.owner().await.is_some()
    }

    /// Start kdeconnectd: D-Bus activation first (its service file), then the
    /// binary. True when it is running or on its way up.
    pub async fn start_daemon(&self) -> bool {
        if self.daemon_running().await {
            return true;
        }
        if self.no_daemon {
            return false;
        }
        if let Ok(p) = zbus::fdo::DBusProxy::new(&self.conn).await {
            if let Ok(name) = SERVICE.try_into() {
                if p.start_service_by_name(name, 0).await.is_ok() {
                    return true;
                }
            }
        }
        for exe in ["/usr/bin/kdeconnectd", "/usr/lib/kdeconnectd"] {
            if std::path::Path::new(exe).is_file() {
                crate::tools::spawn_reaped(exe, &[]);
                return true;
            }
        }
        false
    }

    /// Who a call is addressed to. Normally the service's name — the bus
    /// starts the daemon for it, which is what opening the app should do.
    /// With no-daemon asked, the daemon's own address instead: the bus never
    /// starts anything for one of those, so there is no moment between
    /// "is it running?" and the call in which it could.
    async fn destination(&self) -> Result<String, String> {
        if !self.no_daemon {
            return Ok(SERVICE.to_string());
        }
        self.owner()
            .await
            .ok_or_else(|| "KDE Connect is not running".to_string())
    }

    /// Call a method; the reply's arguments as JSON, in order.
    pub async fn call<B>(
        &self,
        path: &str,
        iface: &str,
        method: &str,
        body: &B,
    ) -> Result<Vec<Json>, String>
    where
        B: serde::Serialize + DynamicType,
    {
        self.call_within(ANSWER, path, iface, method, body).await
    }

    /// `call`, for a method that waits on the phone.
    pub async fn call_within<B>(
        &self,
        limit: Duration,
        path: &str,
        iface: &str,
        method: &str,
        body: &B,
    ) -> Result<Vec<Json>, String>
    where
        B: serde::Serialize + DynamicType,
    {
        let to = self.destination().await?;
        let reply = within(
            limit,
            self.conn
                .call_method(Some(to.as_str()), path, Some(iface), method, body),
        )
        .await?;
        let body = reply.body();
        if body.signature().to_string().is_empty() {
            return Ok(Vec::new());
        }
        let s: Structure<'_> = body.deserialize().map_err(|e| e.to_string())?;
        Ok(s.fields().iter().map(to_json).collect())
    }

    /// The first reply argument, or Null.
    pub async fn call1<B>(
        &self,
        path: &str,
        iface: &str,
        method: &str,
        body: &B,
    ) -> Result<Json, String>
    where
        B: serde::Serialize + DynamicType,
    {
        Ok(self
            .call(path, iface, method, body)
            .await?
            .into_iter()
            .next()
            .unwrap_or(Json::Null))
    }

    /// Every property of one interface.
    pub async fn get_all(&self, path: &str, iface: &str) -> Result<Map<String, Json>, String> {
        let to = self.destination().await?;
        let reply = within(
            ANSWER,
            self.conn
                .call_method(Some(to.as_str()), path, Some(PROPS), "GetAll", &(iface,)),
        )
        .await?;
        let props: HashMap<String, OwnedValue> =
            reply.body().deserialize().map_err(|e| e.to_string())?;
        Ok(props.iter().map(|(k, v)| (k.clone(), to_json(v))).collect())
    }

    pub async fn set(
        &self,
        path: &str,
        iface: &str,
        prop: &str,
        value: Value<'_>,
    ) -> Result<(), String> {
        let to = self.destination().await?;
        within(
            ANSWER,
            self.conn.call_method(
                Some(to.as_str()),
                path,
                Some(PROPS),
                "Set",
                &(iface, prop, value),
            ),
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{Array, Dict, Signature, StructureBuilder};

    #[test]
    fn the_no_daemon_switch() {
        assert!(no_daemon_asked(Some("1".into())));
        assert!(no_daemon_asked(Some("yes".into())));
        assert!(!no_daemon_asked(Some("0".into())));
        assert!(!no_daemon_asked(Some("".into())));
        assert!(!no_daemon_asked(None));
    }

    #[test]
    fn scalars_and_variants() {
        assert_eq!(to_json(&Value::from(7i64)), Json::from(7));
        assert_eq!(to_json(&Value::from(true)), Json::from(true));
        assert_eq!(to_json(&Value::from("hey")), Json::from("hey"));
        let wrapped = Value::Value(Box::new(Value::from(3u8)));
        assert_eq!(to_json(&wrapped), Json::from(3));
    }

    #[test]
    fn struct_becomes_array_and_dict_becomes_object() {
        let s = StructureBuilder::new()
            .add_field(1i32)
            .add_field("body")
            .build()
            .unwrap();
        assert_eq!(
            to_json(&Value::Structure(s)),
            serde_json::json!([1, "body"])
        );

        let mut d = Dict::new(&Signature::Str, &Signature::Variant);
        d.append(
            Value::from("charge"),
            Value::Value(Box::new(Value::from(36i32))),
        )
        .unwrap();
        assert_eq!(
            to_json(&Value::Dict(d)),
            serde_json::json!({ "charge": 36 })
        );

        let a = Array::from(vec!["a", "b"]);
        assert_eq!(to_json(&Value::Array(a)), serde_json::json!(["a", "b"]));
    }
}
