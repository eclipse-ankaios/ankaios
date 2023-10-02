use serde::{Deserialize, Serialize};
#[cfg(not(test))]
use serde_yaml::Value;
use serde_yaml::{Mapping, Serializer};
#[cfg(test)]
use tests::Value;

struct KubeConfig {
    objects: Vec<Value>,
}

impl KubeConfig {
    fn add_label(&mut self, key: String, value: String) -> Result<(), String> {
        for single_object in &mut self.objects {
            let single_object = single_object
                .as_mapping_mut()
                .ok_or_else(|| "Kube object is not a mapping".to_string())?;

            let metadata = single_object
                .entry("metadata".into())
                .or_insert_with(|| Mapping::new().into())
                .as_mapping_mut()
                .ok_or_else(|| "metadata is not a mapping".to_string())?;
            let labels = metadata
                .entry("labels".into())
                .or_insert_with(|| Mapping::new().into())
                .as_mapping_mut()
                .ok_or_else(|| "metadata.labels is not a mapping".to_string())?;
            labels.insert(key.clone().into(), value.clone().into());
        }

        Ok(())
    }

    fn as_bytes(&self) -> Result<Vec<u8>, String> {
        let mut result = Vec::new();
        let mut serializer = Serializer::new(&mut result);
        for single_object in &self.objects {
            single_object
                .serialize(&mut serializer)
                .map_err(|err| format!("Could not serialize Kubernetes YAML: {}", err))?;
        }
        Ok(result)
    }
}

impl TryFrom<&str> for KubeConfig {
    type Error = String;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let objects: Result<Vec<Value>, String> = serde_yaml::Deserializer::from_str(value)
            .map(|document| {
                Value::deserialize(document)
                    .map_err(|err| format!("Could not deserialize Kubernetes YAML: {}", err))
            })
            .collect();

        Ok(KubeConfig { objects: objects? })
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use std::ops::Deref;
    use std::sync::Mutex;

    use serde::ser::Error;
    use serde::Deserialize;
    use serde::Serialize;

    const METADATA_PLACEHOLDER: &str = "{{METADATA}}";

    const SAMPLE_YAML_TEMPLATE_1: &str = concat!(
        "apiVersion: v1\n",
        "kind: ConfigMap\n",
        "data:\n",
        "  key: value\n",
        "{{METADATA}}", // has to be at the end for utest_kube_config_no_previous_metadata
    );

    const SAMPLE_YAML_TEMPLATE_2: &str = concat!(
        "apiVersion: v1\n",
        "kind: Secret\n",
        "{{METADATA}}",
        "data:\n",
        "  .secret-file: c2VjcmV0\n",
    );

    lazy_static::lazy_static! {
        static ref SERIALIZE_SHOULD_FAIL: Mutex<bool> = Mutex::new(false);
        static ref MOCK_VALUE_MTX: Mutex<()> = Mutex::new(());
    }

    #[test]
    fn utest_kube_config_add_label() {
        let _mock_value_mtx = MOCK_VALUE_MTX.lock();
        *SERIALIZE_SHOULD_FAIL.lock().unwrap() = false;

        let sample_1 = SAMPLE_YAML_TEMPLATE_1
            .with_name("sample-config")
            .add_label("label1", "value1");

        let mut kube_config: super::KubeConfig = (*sample_1).try_into().unwrap();

        kube_config
            .add_label("test_label".into(), "test_value".into())
            .unwrap();
        let result = kube_config.as_bytes().unwrap();
        let result = std::str::from_utf8(&result).unwrap();

        let expected_result = sample_1.add_label("test_label", "test_value");

        assert_eq!(*result, *expected_result);
    }

    #[test]
    fn utest_kube_config_add_label_multiple_objects() {
        let _mock_value_mtx = MOCK_VALUE_MTX.lock();
        *SERIALIZE_SHOULD_FAIL.lock().unwrap() = false;

        let sample_1 = SAMPLE_YAML_TEMPLATE_1
            .with_name("sample-config")
            .add_label("label1", "value1");
        let sample_2 = SAMPLE_YAML_TEMPLATE_2
            .with_name("dot-secret")
            .add_label("label2", "value2");

        let mut kube_config: super::KubeConfig =
            (*sample_1.add_document(&sample_2)).try_into().unwrap();

        kube_config
            .add_label("test_label".into(), "test_value".into())
            .unwrap();
        let result = kube_config.as_bytes().unwrap();
        let result = std::str::from_utf8(&result).unwrap();

        let expected_result = sample_1
            .add_label("test_label", "test_value")
            .add_document(&sample_2.add_label("test_label", "test_value"));

        assert_eq!(*result, *expected_result);
    }

    #[test]
    fn utest_kube_config_no_previous_label() {
        let _mock_value_mtx = MOCK_VALUE_MTX.lock();
        *SERIALIZE_SHOULD_FAIL.lock().unwrap() = false;

        let sample_1 = SAMPLE_YAML_TEMPLATE_1.with_name("sample-config");

        let mut kube_config: super::KubeConfig = (*sample_1).try_into().unwrap();

        kube_config
            .add_label("test_label".into(), "test_value".into())
            .unwrap();
        let result = kube_config.as_bytes().unwrap();
        let result = std::str::from_utf8(&result).unwrap();

        let expected_result = sample_1.add_label("test_label", "test_value");

        assert_eq!(*result, *expected_result);
    }

    #[test]
    fn utest_kube_config_no_previous_metadata() {
        let _mock_value_mtx = MOCK_VALUE_MTX.lock();
        *SERIALIZE_SHOULD_FAIL.lock().unwrap() = false;

        let mut kube_config: super::KubeConfig = (*SAMPLE_YAML_TEMPLATE_1.without_metadata())
            .try_into()
            .unwrap();

        kube_config
            .add_label("test_label".into(), "test_value".into())
            .unwrap();
        let result = kube_config.as_bytes().unwrap();
        let result = std::str::from_utf8(&result).unwrap();

        let expected_result = SAMPLE_YAML_TEMPLATE_1
            .with_metadata()
            .add_label("test_label", "test_value");

        assert_eq!(*result, *expected_result);
    }

    #[test]
    fn utest_kube_config_no_yaml() {
        let _mock_value_mtx = MOCK_VALUE_MTX.lock();
        *SERIALIZE_SHOULD_FAIL.lock().unwrap() = false;

        let kube_config: Result<super::KubeConfig, String> = "illegal: yaml\nfile".try_into();
        assert!(
            matches!(kube_config, Err(msg) if msg.starts_with("Could not deserialize Kubernetes YAML:"))
        );
    }

    #[test]
    fn utest_kube_config_no_mapping() {
        let _mock_value_mtx = MOCK_VALUE_MTX.lock();
        *SERIALIZE_SHOULD_FAIL.lock().unwrap() = false;

        let mut kube_config: super::KubeConfig = "42".try_into().unwrap();
        let result = kube_config.add_label("test_label".into(), "test_value".into());
        assert!(matches!(result, Err(msg) if msg == "Kube object is not a mapping"));
    }

    #[test]
    fn utest_kube_config_metadata_no_mapping() {
        let _mock_value_mtx = MOCK_VALUE_MTX.lock();
        *SERIALIZE_SHOULD_FAIL.lock().unwrap() = false;

        let mut kube_config: super::KubeConfig = "metadata: 42".try_into().unwrap();
        let result = kube_config.add_label("test_label".into(), "test_value".into());
        assert!(matches!(result, Err(msg) if msg == "metadata is not a mapping"));
    }

    #[test]
    fn utest_kube_config_labels_no_mapping() {
        let _mock_value_mtx = MOCK_VALUE_MTX.lock();
        *SERIALIZE_SHOULD_FAIL.lock().unwrap() = false;

        let mut kube_config: super::KubeConfig = "metadata:\n  labels: 42".try_into().unwrap();
        let result = kube_config.add_label("test_label".into(), "test_value".into());
        assert!(matches!(result, Err(msg) if msg == "metadata.labels is not a mapping"));
    }

    #[test]
    fn utest_kube_config_cant_be_serialized() {
        let _mock_value_mtx = MOCK_VALUE_MTX.lock();
        *SERIALIZE_SHOULD_FAIL.lock().unwrap() = true;

        let sample_1 = SAMPLE_YAML_TEMPLATE_1
            .with_name("sample-config")
            .add_label("key1", "value1");

        let mut kube_config: super::KubeConfig = (*sample_1).try_into().unwrap();

        kube_config
            .add_label("test_label".into(), "test_value".into())
            .unwrap();
        let result = kube_config.as_bytes();

        assert!(
            matches!(result, Err(msg) if msg.starts_with("Could not serialize Kubernetes YAML:"))
        );
    }

    trait Document {
        fn add_document(&self, document: &str) -> String;
    }

    impl Document for String {
        fn add_document(&self, document: &str) -> String {
            self.to_owned() + "---\n" + document
        }
    }

    trait Config {
        fn without_metadata(&self) -> String;
        fn with_metadata(&self) -> WithMetaData;
        fn with_name(&self, name: &str) -> WithMetaData;
    }

    impl Config for &str {
        fn without_metadata(&self) -> String {
            self.replace(METADATA_PLACEHOLDER, "")
        }

        fn with_metadata(&self) -> WithMetaData {
            WithMetaData::new(*self, "metadata:\n")
        }

        fn with_name(&self, name: &str) -> WithMetaData {
            WithMetaData::new(*self, format!("metadata:\n  name: {}\n", name))
        }
    }

    struct WithMetaData {
        base: String,
        metadata: String,
        labels: Vec<String>,
        result: String,
    }

    impl WithMetaData {
        fn new(base: impl Into<String>, metadata: impl Into<String>) -> Self {
            let mut result = Self {
                base: base.into(),
                metadata: metadata.into(),
                labels: vec![],
                result: "".into(),
            };
            result.update_result();
            result
        }

        fn add_label(mut self, key: &str, value: &str) -> Self {
            let new_line = "    ".to_string() + key + ": " + value + "\n";
            self.labels.push(new_line);
            self.update_result();
            self
        }

        fn update_result(&mut self) {
            let metadata = if self.labels.is_empty() {
                self.metadata.clone()
            } else {
                self.metadata.clone() + "  labels:\n" + &self.labels.join("")
            };
            self.result = self.base.replace(METADATA_PLACEHOLDER, &metadata);
        }
    }

    impl Document for WithMetaData {
        fn add_document(&self, document: &str) -> String {
            self.to_string().add_document(document)
        }
    }

    impl Deref for WithMetaData {
        type Target = str;

        fn deref(&self) -> &Self::Target {
            &self.result
        }
    }

    pub struct Value {
        actual_value: serde_yaml::Value,
    }

    impl Value {
        pub fn as_mapping_mut(&mut self) -> Option<&mut serde_yaml::Mapping> {
            self.actual_value.as_mapping_mut()
        }
    }

    impl Serialize for Value {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            if *SERIALIZE_SHOULD_FAIL.lock().unwrap() {
                Err(S::Error::custom("foobar"))
            } else {
                self.actual_value.serialize(serializer)
            }
        }
    }

    impl<'de> Deserialize<'de> for Value {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            serde_yaml::Value::deserialize(deserializer).map(|value| Value {
                actual_value: value,
            })
        }
    }
}
