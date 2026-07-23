use adss_contract::{
    CredentialEntry, DirectoryOperation, DirectoryOperationKind, DirectoryOperationTarget, Group,
    OrganizationalUnit, User, UserStatus,
};
use async_trait::async_trait;
use ldap3::{Ldap, LdapConnAsync, LdapConnSettings, Mod, Scope, SearchEntry};
use std::collections::HashSet;

use crate::config::{LdapDirectoryConfig, validate_ldap_attribute_name};

use super::{DirectoryClient, DirectoryExecutionContext};
pub struct LdapDirectoryClient {
    config: LdapDirectoryConfig,
}

impl LdapDirectoryClient {
    pub fn new(config: LdapDirectoryConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &LdapDirectoryConfig {
        &self.config
    }

    async fn bind(&self) -> anyhow::Result<Ldap> {
        let settings = LdapConnSettings::new().set_no_tls_verify(self.config.accept_invalid_certs);
        let (connection, mut ldap) =
            LdapConnAsync::with_settings(settings, &self.config.url).await?;
        tokio::spawn(async move {
            if let Err(error) = connection.drive().await {
                eprintln!("ldap connection failed: {error}");
            }
        });
        ldap.simple_bind(&self.config.bind_dn, &self.config.bind_password)
            .await?
            .success()?;
        Ok(ldap)
    }

    async fn ensure_ou(
        &self,
        ldap: &mut Ldap,
        ou: &OrganizationalUnit,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let dn = context.organizational_unit_dn(&ou.id)?;
        if search_base_exists(ldap, dn, "(objectClass=organizationalUnit)").await? {
            ldap.modify(
                dn,
                vec![
                    replace_string("ou", &ou.name),
                    replace_string("name", &ou.name),
                ],
            )
            .await?
            .success()?;
            return Ok(());
        }

        ldap.add(
            dn,
            vec![
                ldap_attr("objectClass", &["top", "organizationalUnit"]),
                ldap_attr("ou", &[&ou.name]),
                ldap_attr("name", &[&ou.name]),
            ],
        )
        .await?
        .success()?;
        Ok(())
    }

    async fn ensure_user(
        &self,
        ldap: &mut Ldap,
        user: &User,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let user_account_control = match user.status {
            UserStatus::Active => "512",
            UserStatus::Disabled => "514",
        };

        if let Some(dn) = find_user_dn(ldap, context, &user.employee_id).await? {
            ldap.modify(
                &dn,
                user_mods(
                    user,
                    &context.domain.employee_id_attribute,
                    &context.domain.upn_suffix,
                    user_account_control,
                ),
            )
            .await?
            .success()?;
            return Ok(());
        }

        let ou_dn = context.organizational_unit_dn(&user.organizational_unit_id)?;
        let dn = format!("CN={},{}", escape_ldap_dn_value(&user.username), ou_dn);
        ldap.add(
            &dn,
            user_attrs(
                user,
                &context.domain.employee_id_attribute,
                &context.domain.upn_suffix,
                "514",
            ),
        )
        .await?
        .success()?;
        Ok(())
    }

    async fn ensure_user_placement(
        &self,
        ldap: &mut Ldap,
        employee_id: &str,
        organizational_unit_id: &str,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let dn = require_user_dn(ldap, context, employee_id).await?;
        let target_parent_dn = context.organizational_unit_dn(organizational_unit_id)?;
        if dn_is_under_parent(&dn, target_parent_dn) {
            return Ok(());
        }
        let rdn = dn_rdn(&dn)?;
        ldap.modifydn(&dn, rdn, true, Some(target_parent_dn))
            .await?
            .success()?;
        Ok(())
    }

    async fn ensure_group(
        &self,
        ldap: &mut Ldap,
        group: &Group,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        if let Some(dn) = find_group_dn(ldap, context, &group.id).await? {
            ldap.modify(
                &dn,
                vec![
                    replace_string("sAMAccountName", &group.name),
                    replace_string("description", &group_marker(&group.id)),
                ],
            )
            .await?
            .success()?;
            return Ok(());
        }

        let dn = format!(
            "CN={},{}",
            escape_ldap_dn_value(&group.name),
            context.domain.mirror_root_dn
        );
        ldap.add(
            &dn,
            vec![
                ldap_attr("objectClass", &["top", "group"]),
                ldap_attr("cn", &[&group.name]),
                ldap_attr("sAMAccountName", &[&group.name]),
                ldap_attr("groupType", &["-2147483646"]),
                ldap_attr("description", &[&group_marker(&group.id)]),
            ],
        )
        .await?
        .success()?;
        Ok(())
    }

    async fn ensure_group_members(
        &self,
        ldap: &mut Ldap,
        group: &Group,
        member_employee_ids: &[String],
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let group_dn = require_group_dn(ldap, context, &group.id).await?;
        let mut member_dns = HashSet::new();
        for employee_id in member_employee_ids {
            member_dns.insert(
                require_user_dn(ldap, context, employee_id)
                    .await?
                    .into_bytes(),
            );
        }
        ldap.modify(
            &group_dn,
            vec![Mod::Replace(b"member".to_vec(), member_dns)],
        )
        .await?
        .success()?;
        Ok(())
    }

    async fn disable_user(
        &self,
        ldap: &mut Ldap,
        employee_id: &str,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let dn = require_user_dn(ldap, context, employee_id).await?;
        ldap.modify(&dn, vec![replace_string("userAccountControl", "514")])
            .await?
            .success()?;
        Ok(())
    }

    async fn move_user_to_quarantine(
        &self,
        ldap: &mut Ldap,
        employee_id: &str,
        quarantine_dn: &str,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let dn = require_user_dn(ldap, context, employee_id).await?;
        if dn_is_under_parent(&dn, quarantine_dn) {
            return Ok(());
        }
        let rdn = dn_rdn(&dn)?;
        ldap.modifydn(&dn, rdn, true, Some(quarantine_dn))
            .await?
            .success()?;
        Ok(())
    }
}

#[async_trait]
impl DirectoryClient for LdapDirectoryClient {
    async fn apply(
        &self,
        operation: &DirectoryOperation,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let mut ldap = self.bind().await?;
        match (operation.kind, operation.target.as_ref()) {
            (
                DirectoryOperationKind::EnsureOu,
                Some(DirectoryOperationTarget::OrganizationalUnit(ou)),
            ) => self.ensure_ou(&mut ldap, ou, context).await,
            (DirectoryOperationKind::EnsureUser, Some(DirectoryOperationTarget::User(user))) => {
                self.ensure_user(&mut ldap, user, context).await
            }
            (
                DirectoryOperationKind::EnsureUserPlacement,
                Some(DirectoryOperationTarget::UserOrganizationalUnitId(ou_id)),
            ) => {
                self.ensure_user_placement(&mut ldap, &operation.subject, ou_id, context)
                    .await
            }
            (DirectoryOperationKind::EnsureGroup, Some(DirectoryOperationTarget::Group(group))) => {
                self.ensure_group(&mut ldap, group, context).await
            }
            (
                DirectoryOperationKind::EnsureGroupMembers,
                Some(DirectoryOperationTarget::GroupMembers {
                    group,
                    member_employee_ids,
                }),
            ) => {
                self.ensure_group_members(&mut ldap, group, member_employee_ids, context)
                    .await
            }
            (DirectoryOperationKind::DisableUser, None) => {
                self.disable_user(&mut ldap, &operation.subject, context)
                    .await
            }
            (
                DirectoryOperationKind::MoveUserToQuarantine,
                Some(DirectoryOperationTarget::QuarantineDn(quarantine_dn)),
            ) => {
                self.move_user_to_quarantine(&mut ldap, &operation.subject, quarantine_dn, context)
                    .await
            }
            _ => anyhow::bail!("directory operation target does not match operation kind"),
        }
    }

    async fn set_password(
        &self,
        credential: &CredentialEntry,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let mut ldap = self.bind().await?;
        let dn = require_user_dn(&mut ldap, context, &credential.employee_id).await?;
        ldap.modify(
            &dn,
            vec![Mod::Replace(
                b"unicodePwd".to_vec(),
                HashSet::from([encode_ad_unicode_password(&credential.plaintext_password)]),
            )],
        )
        .await?
        .success()?;
        let user_account_control = match credential.status {
            UserStatus::Active => "512",
            UserStatus::Disabled => "514",
        };
        ldap.modify(
            &dn,
            vec![replace_string("userAccountControl", user_account_control)],
        )
        .await?
        .success()?;
        Ok(())
    }
}

async fn search_base_exists(ldap: &mut Ldap, dn: &str, filter: &str) -> anyhow::Result<bool> {
    let (entries, _) = ldap
        .search(dn, Scope::Base, filter, vec!["distinguishedName"])
        .await?
        .success()?;
    Ok(!entries.is_empty())
}

async fn find_user_dn(
    ldap: &mut Ldap,
    context: &DirectoryExecutionContext,
    employee_id: &str,
) -> anyhow::Result<Option<String>> {
    validate_ldap_attribute_name(&context.domain.employee_id_attribute)?;
    let filter = format!(
        "(&(objectClass=user)({}={}))",
        context.domain.employee_id_attribute,
        escape_ldap_filter_value(employee_id)
    );
    search_managed_dn(ldap, context, &filter).await
}

async fn require_user_dn(
    ldap: &mut Ldap,
    context: &DirectoryExecutionContext,
    employee_id: &str,
) -> anyhow::Result<String> {
    find_user_dn(ldap, context, employee_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("managed AD user not found for employee {employee_id}"))
}

async fn find_group_dn(
    ldap: &mut Ldap,
    context: &DirectoryExecutionContext,
    group_id: &str,
) -> anyhow::Result<Option<String>> {
    let filter = format!(
        "(&(objectClass=group)(description={}))",
        escape_ldap_filter_value(&group_marker(group_id))
    );
    search_managed_dn(ldap, context, &filter).await
}

async fn require_group_dn(
    ldap: &mut Ldap,
    context: &DirectoryExecutionContext,
    group_id: &str,
) -> anyhow::Result<String> {
    find_group_dn(ldap, context, group_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("managed AD group not found: {group_id}"))
}

async fn search_managed_dn(
    ldap: &mut Ldap,
    context: &DirectoryExecutionContext,
    filter: &str,
) -> anyhow::Result<Option<String>> {
    let mut matched_dn = None;
    for base in managed_search_bases(context) {
        let (entries, _) = ldap
            .search(&base, Scope::Subtree, filter, vec!["distinguishedName"])
            .await?
            .success()?;
        for entry in entries {
            let dn = SearchEntry::construct(entry).dn;
            if matched_dn.is_some() {
                anyhow::bail!("managed AD search returned more than one object for {filter}");
            }
            matched_dn = Some(dn);
        }
    }
    Ok(matched_dn)
}

fn managed_search_bases(context: &DirectoryExecutionContext) -> Vec<String> {
    if context.domain.quarantine_ou_dn == context.domain.mirror_root_dn {
        vec![context.domain.mirror_root_dn.clone()]
    } else {
        vec![
            context.domain.mirror_root_dn.clone(),
            context.domain.quarantine_ou_dn.clone(),
        ]
    }
}

fn group_marker(group_id: &str) -> String {
    format!("adss:group_id:{group_id}")
}

fn user_attrs(
    user: &User,
    employee_id_attribute: &str,
    upn_suffix: &str,
    user_account_control: &str,
) -> Vec<(Vec<u8>, HashSet<Vec<u8>>)> {
    let mut attrs = vec![
        ldap_attr(
            "objectClass",
            &["top", "person", "organizationalPerson", "user"],
        ),
        ldap_attr("cn", &[&user.username]),
        ldap_attr("sn", &[&user.display_name]),
        ldap_attr("displayName", &[&user.display_name]),
        ldap_attr("sAMAccountName", &[&user.username]),
        ldap_attr(
            "userPrincipalName",
            &[&format!("{}@{}", user.username, upn_suffix)],
        ),
        ldap_attr(employee_id_attribute, &[&user.employee_id]),
        ldap_attr("userAccountControl", &[user_account_control]),
    ];
    push_optional_attr(&mut attrs, "mail", user.email.as_deref());
    push_optional_attr(&mut attrs, "mobile", user.mobile.as_deref());
    push_optional_attr(&mut attrs, "telephoneNumber", user.telephone.as_deref());
    attrs
}

fn user_mods(
    user: &User,
    employee_id_attribute: &str,
    upn_suffix: &str,
    user_account_control: &str,
) -> Vec<Mod<Vec<u8>>> {
    vec![
        replace_string("sn", &user.display_name),
        replace_string("displayName", &user.display_name),
        replace_string("sAMAccountName", &user.username),
        replace_string(
            "userPrincipalName",
            &format!("{}@{}", user.username, upn_suffix),
        ),
        replace_string(employee_id_attribute, &user.employee_id),
        replace_string("userAccountControl", user_account_control),
        replace_optional("mail", user.email.as_deref()),
        replace_optional("mobile", user.mobile.as_deref()),
        replace_optional("telephoneNumber", user.telephone.as_deref()),
    ]
}

fn ldap_attr(name: &str, values: &[&str]) -> (Vec<u8>, HashSet<Vec<u8>>) {
    (
        name.as_bytes().to_vec(),
        values
            .iter()
            .map(|value| value.as_bytes().to_vec())
            .collect(),
    )
}

fn push_optional_attr(
    attrs: &mut Vec<(Vec<u8>, HashSet<Vec<u8>>)>,
    name: &str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        attrs.push(ldap_attr(name, &[value]));
    }
}

fn replace_string(name: &str, value: &str) -> Mod<Vec<u8>> {
    Mod::Replace(
        name.as_bytes().to_vec(),
        HashSet::from([value.as_bytes().to_vec()]),
    )
}

fn replace_optional(name: &str, value: Option<&str>) -> Mod<Vec<u8>> {
    Mod::Replace(
        name.as_bytes().to_vec(),
        value
            .map(|value| HashSet::from([value.as_bytes().to_vec()]))
            .unwrap_or_default(),
    )
}

fn dn_is_under_parent(dn: &str, parent_dn: &str) -> bool {
    dn.len() > parent_dn.len()
        && dn.ends_with(parent_dn)
        && dn.as_bytes().get(dn.len() - parent_dn.len() - 1).copied() == Some(b',')
}

fn dn_rdn(dn: &str) -> anyhow::Result<&str> {
    let mut escaped = false;
    for (index, character) in dn.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if character == ',' {
            return Ok(&dn[..index]);
        }
    }
    anyhow::bail!("DN has no parent component: {dn}")
}

pub fn escape_ldap_filter_value(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '*' => escaped.push_str(r"\2a"),
            '(' => escaped.push_str(r"\28"),
            ')' => escaped.push_str(r"\29"),
            '\\' => escaped.push_str(r"\5c"),
            '\0' => escaped.push_str(r"\00"),
            _ => escaped.push(character),
        }
    }
    escaped
}

pub fn escape_ldap_dn_value(value: &str) -> String {
    let mut escaped = String::new();
    let mut chars = value.char_indices().peekable();

    while let Some((index, character)) = chars.next() {
        let is_first = index == 0;
        let is_last = chars.peek().is_none();
        if (is_first && (character == ' ' || character == '#'))
            || (is_last && character == ' ')
            || matches!(character, ',' | '+' | '"' | '\\' | '<' | '>' | ';' | '=')
        {
            escaped.push('\\');
        }
        escaped.push(character);
    }

    escaped
}

pub fn encode_ad_unicode_password(password: &str) -> Vec<u8> {
    format!("\"{password}\"")
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect()
}
