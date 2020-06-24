use crate::file::{FileFormat, SaveDir, SerializedFile};
use anime::remote::{AccessToken, Remote};
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents all (non-offline) remote types from the anime library.
///
/// When dealing with users, this type should be used instead of the
/// `Remote` type from the anime library as it does not make sense to
/// associate a user with an offline service.
#[derive(Copy, Clone, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteType {
    AniList,
}

impl RemoteType {
    /// Returns the name of current `RemoteType`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AniList => "AniList",
        }
    }

    /// Returns all `RemoteType` variants.
    #[inline(always)]
    pub fn all() -> &'static [Self] {
        &[Self::AniList]
    }
}

/// Unique user information for a remote service.
#[derive(Clone, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct UserInfo {
    /// The remote service the user is registered on.
    pub service: RemoteType,
    /// The user's name on the remote service.
    pub username: String,
}

impl UserInfo {
    pub fn new<S>(service: RemoteType, username: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            service,
            username: username.into(),
        }
    }

    pub fn is_logged_in(&self, remote: &Remote) -> bool {
        use anime::remote::anilist::AniList;

        match (self.service, remote) {
            (RemoteType::AniList, Remote::AniList(anilist)) => match anilist {
                AniList::Authenticated(auth) => auth.user.name == self.username,
                AniList::Unauthenticated => false,
            },
            (RemoteType::AniList, Remote::Offline(_)) => false,
        }
    }
}

pub type UserMap = HashMap<UserInfo, AccessToken>;

/// A map containing all users along with the last used one.
#[derive(Default, Deserialize, Serialize)]
pub struct Users {
    users: UserMap,
    pub last_used: Option<UserInfo>,
}

impl Users {
    #[cfg(test)]
    pub fn new() -> Self {
        Self {
            users: UserMap::new(),
            last_used: None,
        }
    }

    /// Adds a new (unique) `user` to the user map and sets the last used user to `user`.
    pub fn add_and_set_last(&mut self, user: UserInfo, token: AccessToken) {
        self.last_used = Some(user.clone());
        self.users.insert(user, token);
    }

    /// Removes the specified `user` from the user map.
    ///
    /// This also unsets the last used user if it was set to `user`.
    pub fn remove(&mut self, user: &UserInfo) {
        self.users.remove(user);

        if let Some(last) = &self.last_used {
            if user == last {
                self.last_used = None;
            }
        }
    }

    /// Returns the last used user's access token if it was set.
    pub fn take_last_used_token(mut self) -> Option<AccessToken> {
        let last = self.last_used?;
        self.users.remove(&last)
    }

    #[inline(always)]
    pub fn get(&self) -> &UserMap {
        &self.users
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.users.len()
    }
}

impl SerializedFile for Users {
    fn filename() -> &'static str {
        "users"
    }

    fn save_dir() -> SaveDir {
        SaveDir::LocalData
    }

    fn format() -> FileFormat {
        FileFormat::MessagePack
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_users() {
        let mut users = Users::new();

        let user1 = UserInfo::new(RemoteType::AniList, "User 1");
        let user1_duplicate = user1.clone();

        users.add_and_set_last(user1, AccessToken::encode("token1"));
        users.add_and_set_last(user1_duplicate, AccessToken::encode("token2"));

        assert_eq!(users.len(), 1);

        let user2 = UserInfo::new(RemoteType::AniList, "User 2");
        users.add_and_set_last(user2, AccessToken::encode("token3"));

        assert_eq!(users.len(), 2);
    }
}
