use gtk::gio;
use gtk::glib::{
    MainContext, Variant,
    variant::{ObjectPath, ToVariant},
};
use linguamesh_domain::{ErrorKind, SecretRef, SecretValue, TranslationError};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant};

const SERVICE_NAME: &str = "org.freedesktop.secrets";
const SERVICE_PATH: &str = "/org/freedesktop/secrets";
const SERVICE_INTERFACE: &str = "org.freedesktop.Secret.Service";
const COLLECTION_INTERFACE: &str = "org.freedesktop.Secret.Collection";
const ITEM_INTERFACE: &str = "org.freedesktop.Secret.Item";
const PROMPT_INTERFACE: &str = "org.freedesktop.Secret.Prompt";
const CALL_TIMEOUT_MS: i32 = 5_000;
const PROMPT_TIMEOUT: Duration = Duration::from_secs(300);
const SECRET_ATTRIBUTE: &str = "linguamesh-secret-ref";
const SECRET_LABEL: &str = "LinguaMesh provider credential";

// Secret Service 的 plain 会话参数必须是单层字符串变体。
fn open_session_parameters() -> Variant {
    ("plain", "".to_variant()).to_variant()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LookupError {
    Unavailable,
    Missing,
    Locked,
    PromptDismissed,
}

struct SecretSession {
    connection: gio::DBusConnection,
    path: ObjectPath,
}

impl SecretSession {
    fn open() -> Result<Self, LookupError> {
        let connection = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>)
            .map_err(|_| LookupError::Unavailable)?;
        let parameters = open_session_parameters();
        let response = connection
            .call_sync(
                Some(SERVICE_NAME),
                SERVICE_PATH,
                SERVICE_INTERFACE,
                "OpenSession",
                Some(&parameters),
                None,
                gio::DBusCallFlags::NONE,
                CALL_TIMEOUT_MS,
                None::<&gio::Cancellable>,
            )
            .map_err(|_| LookupError::Unavailable)?;
        let (_, path): (Variant, ObjectPath) = response.get().ok_or(LookupError::Unavailable)?;
        Ok(Self { connection, path })
    }

    fn call(
        &self,
        object_path: &str,
        interface: &str,
        method: &str,
        parameters: &Variant,
    ) -> Result<Variant, LookupError> {
        self.connection
            .call_sync(
                Some(SERVICE_NAME),
                object_path,
                interface,
                method,
                Some(parameters),
                None,
                gio::DBusCallFlags::NONE,
                CALL_TIMEOUT_MS,
                None::<&gio::Cancellable>,
            )
            .map_err(|_| LookupError::Unavailable)
    }

    // 调用 Secret Service 提示对象并等待用户完成或拒绝交互。
    fn prompt(&self, prompt: &ObjectPath) -> Result<(), LookupError> {
        if prompt.as_str() == "/" {
            return Ok(());
        }
        let completed = Rc::new(Cell::new(None));
        let completed_result = Rc::clone(&completed);
        let subscription = self.connection.subscribe_to_signal(
            Some(SERVICE_NAME),
            Some(PROMPT_INTERFACE),
            Some("Completed"),
            Some(prompt.as_str()),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                let Some((dismissed, _result)) = signal.parameters.get::<(bool, Variant)>() else {
                    return;
                };
                completed_result.set(Some(dismissed));
            },
        );
        let parameters = ("",).to_variant();
        self.connection
            .call_sync(
                Some(SERVICE_NAME),
                prompt.as_str(),
                PROMPT_INTERFACE,
                "Prompt",
                Some(&parameters),
                None,
                gio::DBusCallFlags::NONE,
                CALL_TIMEOUT_MS,
                None::<&gio::Cancellable>,
            )
            .map_err(|_| LookupError::Unavailable)?;

        let context = MainContext::default();
        let deadline = Instant::now() + PROMPT_TIMEOUT;
        while completed.get().is_none() && Instant::now() < deadline {
            let dispatched = context.iteration(false);
            if !dispatched {
                thread::sleep(Duration::from_millis(10));
            }
        }
        drop(subscription);
        match completed.get() {
            Some(false) => Ok(()),
            Some(true) => Err(LookupError::PromptDismissed),
            None => Err(LookupError::Unavailable),
        }
    }
}

impl Drop for SecretSession {
    fn drop(&mut self) {
        let parameters = (&self.path,).to_variant();
        let _ = self.connection.call_sync(
            Some(SERVICE_NAME),
            SERVICE_PATH,
            SERVICE_INTERFACE,
            "CloseSession",
            Some(&parameters),
            None,
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MS,
            None::<&gio::Cancellable>,
        );
    }
}

fn attributes(secret_ref: &SecretRef) -> BTreeMap<String, String> {
    BTreeMap::from([(SECRET_ATTRIBUTE.to_owned(), secret_ref.as_str().to_owned())])
}

fn properties(secret_ref: &SecretRef) -> BTreeMap<String, Variant> {
    let attributes = attributes(secret_ref).to_variant();
    BTreeMap::from([
        (
            "org.freedesktop.Secret.Item.Label".to_owned(),
            SECRET_LABEL.to_variant(),
        ),
        (
            "org.freedesktop.Secret.Item.Attributes".to_owned(),
            attributes,
        ),
    ])
}

fn search_items(
    session: &SecretSession,
    secret_ref: &SecretRef,
) -> Result<(Vec<ObjectPath>, Vec<ObjectPath>), LookupError> {
    let parameters = (attributes(secret_ref),).to_variant();
    let response = session.call(SERVICE_PATH, SERVICE_INTERFACE, "SearchItems", &parameters)?;
    response.get().ok_or(LookupError::Unavailable)
}

// 通过 Secret Service 的 default 别名解析实际的密钥集合对象路径。
fn default_collection(session: &SecretSession) -> Result<ObjectPath, LookupError> {
    let parameters = ("default",).to_variant();
    let response = session.call(SERVICE_PATH, SERVICE_INTERFACE, "ReadAlias", &parameters)?;
    let (collection,): (ObjectPath,) = response.get().ok_or(LookupError::Unavailable)?;
    Ok(collection)
}

fn secret_tuple(
    session: &SecretSession,
    secret: &SecretValue,
) -> (ObjectPath, Vec<u8>, Vec<u8>, String) {
    (
        session.path.clone(),
        Vec::new(),
        secret.expose_secret().as_bytes().to_vec(),
        "text/plain".to_owned(),
    )
}

pub fn store_secret(secret_ref: &SecretRef, secret: &SecretValue) -> Result<(), TranslationError> {
    if !secret_ref.is_persistent() {
        return Err(TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "Only persistent secret references may use Secret Service.",
        ));
    }
    let session = SecretSession::open().map_err(map_store_error)?;
    let (unlocked, locked) = search_items(&session, secret_ref).map_err(map_store_error)?;
    if let Some(item) = unlocked.first() {
        let parameters = (secret_tuple(&session, secret),).to_variant();
        session
            .call(item, ITEM_INTERFACE, "SetSecret", &parameters)
            .map_err(map_store_error)?;
        return Ok(());
    }
    if !locked.is_empty() {
        return Err(map_store_error(LookupError::Locked));
    }
    let collection = default_collection(&session).map_err(map_store_error)?;
    let parameters = (
        properties(secret_ref),
        secret_tuple(&session, secret),
        false,
    )
        .to_variant();
    let response = session
        .call(
            collection.as_str(),
            COLLECTION_INTERFACE,
            "CreateItem",
            &parameters,
        )
        .map_err(map_store_error)?;
    let (_, prompt): (ObjectPath, ObjectPath) = response
        .get()
        .ok_or_else(|| map_store_error(LookupError::Unavailable))?;
    session.prompt(&prompt).map_err(map_store_error)?;
    Ok(())
}

pub fn resolve_secret(secret_ref: &SecretRef) -> Result<SecretValue, LookupError> {
    if !secret_ref.is_persistent() {
        return Err(LookupError::Missing);
    }
    let session = SecretSession::open()?;
    let (unlocked, locked) = search_items(&session, secret_ref)?;
    let Some(item) = unlocked.first() else {
        if !locked.is_empty() {
            return Err(LookupError::Locked);
        }
        return Err(LookupError::Missing);
    };
    let parameters = (&session.path,).to_variant();
    let response = session.call(item, ITEM_INTERFACE, "GetSecret", &parameters)?;
    let ((_, _, value, _),): ((ObjectPath, Vec<u8>, Vec<u8>, String),) =
        response.get().ok_or(LookupError::Unavailable)?;
    let value = String::from_utf8(value).map_err(|_| LookupError::Unavailable)?;
    Ok(SecretValue::new(value))
}

pub fn delete_secret(secret_ref: &SecretRef) -> Result<(), TranslationError> {
    if !secret_ref.is_persistent() {
        return Ok(());
    }
    let session = SecretSession::open().map_err(map_store_error)?;
    let (unlocked, locked) = search_items(&session, secret_ref).map_err(map_store_error)?;
    if !locked.is_empty() && unlocked.is_empty() {
        return Err(map_store_error(LookupError::Locked));
    }
    for item in unlocked {
        let response = session
            .call(&item, ITEM_INTERFACE, "Delete", &().to_variant())
            .map_err(map_store_error)?;
        let (prompt,): (ObjectPath,) = response
            .get()
            .ok_or_else(|| map_store_error(LookupError::Unavailable))?;
        session.prompt(&prompt).map_err(map_store_error)?;
    }
    Ok(())
}

fn map_store_error(error: LookupError) -> TranslationError {
    let message = match error {
        LookupError::Unavailable => "Secret Service is unavailable.",
        LookupError::Missing => "The provider credential is not stored in Secret Service.",
        LookupError::Locked => "The Secret Service item is locked.",
        LookupError::PromptDismissed => "The Secret Service prompt was dismissed.",
    };
    TranslationError::new(ErrorKind::SecureStorageUnavailable, message)
}

#[cfg(test)]
mod tests {
    use super::{
        LookupError, SECRET_ATTRIBUTE, attributes, delete_secret, open_session_parameters,
        properties, resolve_secret, store_secret,
    };
    use gtk::glib::VariantTy;
    use linguamesh_domain::{ErrorKind, SecretRef, SecretRefNamespace, SecretValue};

    const PERSISTENT_FIXTURE_REF: &str = "secret-service:11111111-1111-4111-8111-111111111111";

    fn persistent_fixture_ref() -> SecretRef {
        SecretRef::parse(PERSISTENT_FIXTURE_REF).expect("persistent fixture reference")
    }

    #[test]
    fn attributes_use_only_the_secret_reference() {
        let secret_ref = SecretRef::new(SecretRefNamespace::SecretService);
        let attributes = attributes(&secret_ref);
        assert_eq!(
            attributes.get(SECRET_ATTRIBUTE),
            Some(&secret_ref.as_str().to_owned())
        );
        let properties = properties(&secret_ref);
        assert!(properties.contains_key("org.freedesktop.Secret.Item.Label"));
        assert!(properties.contains_key("org.freedesktop.Secret.Item.Attributes"));
    }

    #[test]
    fn open_session_parameters_wrap_only_the_plain_string() {
        let parameters = open_session_parameters();
        assert_eq!(parameters.type_(), VariantTy::new("(sv)").unwrap());
        let mechanism: String = parameters.child_get(0);
        let input: gtk::glib::Variant = parameters.child_get(1);
        assert_eq!(mechanism, "plain");
        assert_eq!(input.type_(), VariantTy::STRING);
    }

    #[test]
    #[ignore = "requires the isolated Secret Service fixture"]
    fn secret_service_round_trip_and_cleanup() {
        let secret_ref = SecretRef::new(SecretRefNamespace::SecretService);
        let secret = SecretValue::new("linguamesh-ci-secret");
        store_secret(&secret_ref, &secret).expect("store secret");
        let resolved = resolve_secret(&secret_ref);
        let cleanup = delete_secret(&secret_ref);
        cleanup.expect("delete secret");
        assert_eq!(
            resolved.expect("resolve secret").expose_secret(),
            secret.expose_secret()
        );
        assert!(matches!(
            resolve_secret(&secret_ref),
            Err(LookupError::Missing)
        ));
    }

    #[test]
    #[ignore = "requires the persistent Secret Service fixture"]
    fn secret_service_persistent_store_for_restart() {
        let secret_ref = persistent_fixture_ref();
        let secret = SecretValue::new("linguamesh-persistent-fixture");
        store_secret(&secret_ref, &secret).expect("store persistent secret");
        assert_eq!(
            resolve_secret(&secret_ref)
                .expect("resolve persistent secret")
                .expose_secret(),
            secret.expose_secret()
        );
    }

    #[test]
    #[ignore = "requires a locked persistent Secret Service fixture"]
    fn secret_service_locked_item_fails_closed() {
        let secret_ref = persistent_fixture_ref();
        assert!(matches!(
            resolve_secret(&secret_ref),
            Err(LookupError::Locked)
        ));
    }

    #[test]
    #[ignore = "requires the isolated Secret Service prompt fixture"]
    fn secret_service_prompt_is_accepted_when_storing() {
        let secret_ref = persistent_fixture_ref();
        let secret = SecretValue::new("linguamesh-prompt-fixture");
        store_secret(&secret_ref, &secret).expect("approved prompted store");
    }

    #[test]
    #[ignore = "requires the isolated Secret Service prompt fixture"]
    fn secret_service_prompt_is_accepted_when_deleting() {
        let secret_ref = persistent_fixture_ref();
        delete_secret(&secret_ref).expect("approved prompted delete");
    }

    #[test]
    #[ignore = "requires the isolated Secret Service prompt fixture"]
    fn secret_service_prompt_is_rejected_when_storing() {
        let secret_ref = persistent_fixture_ref();
        let secret = SecretValue::new("linguamesh-prompt-fixture");
        let error =
            store_secret(&secret_ref, &secret).expect_err("prompted store must fail closed");
        assert_eq!(error.kind, ErrorKind::SecureStorageUnavailable);
        assert_eq!(error.message, "The Secret Service prompt was dismissed.");
    }

    #[test]
    #[ignore = "requires the isolated Secret Service prompt fixture"]
    fn secret_service_prompt_is_rejected_when_deleting() {
        let secret_ref = persistent_fixture_ref();
        let error = delete_secret(&secret_ref).expect_err("prompted delete must fail closed");
        assert_eq!(error.kind, ErrorKind::SecureStorageUnavailable);
        assert_eq!(error.message, "The Secret Service prompt was dismissed.");
    }

    #[test]
    #[ignore = "requires the persistent Secret Service fixture after daemon restart"]
    fn secret_service_persistent_resolves_after_daemon_restart() {
        let secret_ref = persistent_fixture_ref();
        let resolved = resolve_secret(&secret_ref).expect("resolve after daemon restart");
        assert_eq!(resolved.expose_secret(), "linguamesh-persistent-fixture");
        delete_secret(&secret_ref).expect("delete persistent secret");
        assert!(matches!(
            resolve_secret(&secret_ref),
            Err(LookupError::Missing)
        ));
    }
}
