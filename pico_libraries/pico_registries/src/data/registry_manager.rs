use crate::data::registry::{NbtTagData, Registry};
use crate::registry_keys::RegistryKeys;
use pico_nbt::{NbtOptions, Value, from_path_with_options, from_value};
use std::collections::HashMap;
use std::path::Path;
use tracing::debug;

pub struct RegistryManager {
    registries: HashMap<RegistryKeys, Registry>,
}

impl RegistryManager {
    #[must_use]
    pub const fn builder() -> RegistryManagerBuilder {
        RegistryManagerBuilder::new()
    }

    /// Get a registry
    ///
    /// # Errors
    /// Returns an error if the registry is not found
    pub fn get(&self, registry_ref: &RegistryKeys) -> crate::Result<&Registry> {
        self.registries
            .get(registry_ref)
            .ok_or(crate::Error::UnknownRegistry)
    }

    #[must_use]
    pub fn try_get(&self, registry_ref: &RegistryKeys) -> Option<&Registry> {
        self.registries.get(registry_ref)
    }
}

pub struct RegistryManagerBuilder {
    registry_keys: Vec<RegistryKeys>,
}

impl RegistryManagerBuilder {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            registry_keys: Vec::new(),
        }
    }

    /// Register a single registry key
    #[must_use]
    pub fn register(mut self, key: RegistryKeys) -> Self {
        self.registry_keys.push(key);
        self
    }

    /// Register multiple registry keys at once
    #[must_use]
    pub fn register_all(mut self, keys: &[RegistryKeys]) -> Self {
        self.registry_keys.extend_from_slice(keys);
        self
    }

    /// Build the `RegistryManager` by loading all registered registries from the resource path.
    ///
    /// # Errors
    /// Returns an error if a mandatory registry cannot be loaded.
    pub fn load_from_resource_path(self, resource_path: &Path) -> crate::Result<RegistryManager> {
        if resource_path.join("registries.nbt").exists() {
            return self.load_from_nbt_files(resource_path);
        }
        let data_path = resource_path.join("data");
        let mut registries = HashMap::new();
        for registry_key in &self.registry_keys {
            match Registry::load(registry_key, &data_path) {
                Ok(registry) => {
                    registries.insert(registry_key.clone(), registry);
                }
                Err(error) if registry_key.is_mandatory() => return Err(error),
                Err(_) => {
                    debug!(
                        registry_key = ?registry_key,
                        "Failed to load optional registry, skipping"
                    );
                }
            }
        }
        Ok(RegistryManager { registries })
    }

    fn load_from_nbt_files(self, resource_path: &Path) -> crate::Result<RegistryManager> {
        let options = NbtOptions::new().nameless_root(true);
        let (_, data) = from_path_with_options(resource_path.join("registries.nbt"), options)?;
        let Value::Compound(mut data) = data else {
            return Err(crate::Error::Nbt);
        };
        let (_, tags) = from_path_with_options(resource_path.join("tags.nbt"), options)?;
        let mut tags: HashMap<String, NbtTagData> = from_value(tags)?;
        let mut registries = HashMap::new();
        for key in self.registry_keys {
            let id = key.id().to_string();
            let Some(data) = data.swap_remove(&id) else {
                if key.is_mandatory() {
                    return Err(crate::Error::UnknownRegistry);
                }
                continue;
            };
            let registry = Registry::from_nbt(&key, data, tags.remove(&id))?;
            registries.insert(key, registry);
        }
        Ok(RegistryManager { registries })
    }

    /// Register the default set of registry keys
    #[must_use]
    pub fn with_defaults(self) -> Self {
        self.register_all(RegistryKeys::ALL_REGISTRIES)
    }
}

impl Default for RegistryManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}
