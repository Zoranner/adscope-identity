use adss_contract::{
    AgentConfirmRequest, CredentialBatch, CredentialEntry, DirectoryBatch, DirectoryOperationKind,
    DirectoryOperationTarget, DirectoryPlan, DirectoryPlanError, DomainDirectoryConfig, Group,
    OrganizationalUnit, SyncChannel, User, UserStatus,
};
use serde::{
    Serialize,
    ser::{self, Impossible, SerializeSeq, SerializeStruct, Serializer},
};
use std::{error::Error, fmt};

#[test]
fn directory_plan_orders_current_objects_by_ad_dependencies() {
    let batch = DirectoryBatch {
        server_revision: 7,
        batch_revision: 7,
        organizational_units: vec![OrganizationalUnit {
            id: "ou-rd".to_string(),
            name: "研发部".to_string(),
            parent_id: None,
            changed_revision: 7,
        }],
        users: vec![User {
            employee_id: "1001".to_string(),
            username: "zhangsan".to_string(),
            display_name: "张三".to_string(),
            email: Some("zhangsan@example.com".to_string()),
            mobile: Some("13800000000".to_string()),
            telephone: None,
            organizational_unit_id: "ou-rd".to_string(),
            status: UserStatus::Active,
            changed_revision: 7,
        }],
        groups: vec![Group {
            id: "dev".to_string(),
            name: "Developers".to_string(),
            member_employee_ids: vec!["1001".to_string()],
            changed_revision: 7,
        }],
        has_more: false,
    };

    let plan = DirectoryPlan::try_from_batch(&batch, &DomainDirectoryConfig::example()).unwrap();
    let kinds: Vec<_> = plan
        .operations
        .iter()
        .map(|operation| operation.kind)
        .collect();

    assert_eq!(
        kinds,
        vec![
            DirectoryOperationKind::EnsureOu,
            DirectoryOperationKind::EnsureUser,
            DirectoryOperationKind::EnsureUserPlacement,
            DirectoryOperationKind::EnsureGroup,
            DirectoryOperationKind::EnsureGroupMembers,
        ]
    );
    assert_eq!(
        plan.operations[0].target,
        Some(DirectoryOperationTarget::OrganizationalUnit(
            batch.organizational_units[0].clone()
        ))
    );
    assert_eq!(
        plan.operations[1].target,
        Some(DirectoryOperationTarget::User(batch.users[0].clone()))
    );
    assert_eq!(
        plan.operations[2].target,
        Some(DirectoryOperationTarget::UserOrganizationalUnitId(
            "ou-rd".to_string()
        ))
    );
    assert_eq!(
        plan.operations[3].target,
        Some(DirectoryOperationTarget::Group(batch.groups[0].clone()))
    );
    assert_eq!(
        plan.operations[4].target,
        Some(DirectoryOperationTarget::GroupMembers {
            group: batch.groups[0].clone(),
            member_employee_ids: vec!["1001".to_string()]
        })
    );
}

#[test]
fn directory_plan_orders_parent_ou_before_child_ou() {
    let batch = DirectoryBatch {
        server_revision: 8,
        batch_revision: 8,
        organizational_units: vec![
            OrganizationalUnit {
                id: "ou-child".to_string(),
                name: "研发二部".to_string(),
                parent_id: Some("ou-parent".to_string()),
                changed_revision: 8,
            },
            OrganizationalUnit {
                id: "ou-parent".to_string(),
                name: "研发部".to_string(),
                parent_id: None,
                changed_revision: 8,
            },
        ],
        users: Vec::new(),
        groups: Vec::new(),
        has_more: false,
    };

    let plan = DirectoryPlan::try_from_batch(&batch, &DomainDirectoryConfig::example()).unwrap();
    let subjects: Vec<_> = plan
        .operations
        .iter()
        .map(|operation| operation.subject.as_str())
        .collect();

    assert_eq!(subjects, vec!["ou-parent", "ou-child"]);
    assert_eq!(
        plan.operations[1].target,
        Some(DirectoryOperationTarget::OrganizationalUnit(
            batch.organizational_units[0].clone()
        ))
    );
}

#[test]
fn directory_plan_rejects_duplicate_ou_ids() {
    let batch = DirectoryBatch {
        server_revision: 8,
        batch_revision: 8,
        organizational_units: vec![
            OrganizationalUnit {
                id: "ou-rd".to_string(),
                name: "研发部".to_string(),
                parent_id: None,
                changed_revision: 8,
            },
            OrganizationalUnit {
                id: "ou-rd".to_string(),
                name: "研发中心".to_string(),
                parent_id: None,
                changed_revision: 8,
            },
        ],
        users: Vec::new(),
        groups: Vec::new(),
        has_more: false,
    };

    let error = DirectoryPlan::try_from_batch(&batch, &DomainDirectoryConfig::example())
        .expect_err("duplicate OU IDs must fail planning");

    assert_eq!(
        error,
        DirectoryPlanError::DuplicateOuId("ou-rd".to_string())
    );
}

#[test]
fn directory_plan_rejects_cyclic_ou_hierarchy() {
    let batch = DirectoryBatch {
        server_revision: 8,
        batch_revision: 8,
        organizational_units: vec![
            OrganizationalUnit {
                id: "ou-a".to_string(),
                name: "A".to_string(),
                parent_id: Some("ou-b".to_string()),
                changed_revision: 8,
            },
            OrganizationalUnit {
                id: "ou-b".to_string(),
                name: "B".to_string(),
                parent_id: Some("ou-a".to_string()),
                changed_revision: 8,
            },
        ],
        users: Vec::new(),
        groups: Vec::new(),
        has_more: false,
    };

    let error = DirectoryPlan::try_from_batch(&batch, &DomainDirectoryConfig::example())
        .expect_err("cyclic OU hierarchy must fail planning");

    assert_eq!(
        error,
        DirectoryPlanError::CyclicOuHierarchy("ou-a".to_string())
    );
}

#[test]
fn disabled_users_are_ensured_then_disabled_and_moved_to_quarantine() {
    let batch = DirectoryBatch {
        server_revision: 9,
        batch_revision: 9,
        organizational_units: Vec::new(),
        users: vec![User {
            employee_id: "1002".to_string(),
            username: "lisi".to_string(),
            display_name: "李四".to_string(),
            email: None,
            mobile: None,
            telephone: None,
            organizational_unit_id: "ou-rd".to_string(),
            status: UserStatus::Disabled,
            changed_revision: 9,
        }],
        groups: Vec::new(),
        has_more: false,
    };

    let config = DomainDirectoryConfig::example();
    let plan = DirectoryPlan::try_from_batch(&batch, &config).unwrap();
    let kinds: Vec<_> = plan
        .operations
        .iter()
        .map(|operation| operation.kind)
        .collect();

    assert_eq!(
        kinds,
        vec![
            DirectoryOperationKind::EnsureUser,
            DirectoryOperationKind::DisableUser,
            DirectoryOperationKind::MoveUserToQuarantine,
        ]
    );
    assert_eq!(
        plan.operations[0].target,
        Some(DirectoryOperationTarget::User(batch.users[0].clone()))
    );
    assert_eq!(
        plan.operations[2].target,
        Some(DirectoryOperationTarget::QuarantineDn(
            config.quarantine_ou_dn
        ))
    );
}

#[test]
fn credential_debug_output_does_not_include_password_payload() {
    let batch = CredentialBatch {
        server_revision: 12,
        batch_revision: 12,
        credentials: vec![CredentialEntry {
            employee_id: "1001".to_string(),
            plaintext_password: "Secret123!".to_string(),
            status: UserStatus::Active,
            changed_revision: 12,
        }],
        has_more: false,
    };

    let output = format!("{batch:?}");

    assert!(!output.contains("Secret123!"));
    assert!(output.contains("[redacted]"));
}

#[test]
fn credential_wire_json_includes_password_payload() {
    let batch = CredentialBatch {
        server_revision: 12,
        batch_revision: 12,
        credentials: vec![CredentialEntry {
            employee_id: "1001".to_string(),
            plaintext_password: "Secret123!".to_string(),
            status: UserStatus::Active,
            changed_revision: 12,
        }],
        has_more: false,
    };

    let output = to_test_json(&batch);

    assert!(output.contains("\"plaintext_password\":\"Secret123!\""));
}

#[test]
fn paged_batches_are_confirmed_at_batch_revision_not_server_revision() {
    let batch = DirectoryBatch {
        server_revision: 20,
        batch_revision: 12,
        organizational_units: Vec::new(),
        users: Vec::new(),
        groups: Vec::new(),
        has_more: true,
    };

    let confirm = AgentConfirmRequest {
        domain_id: "domain-a".to_string(),
        channel: SyncChannel::Directory,
        target_revision: batch.confirm_revision(),
        success: true,
        error_code: None,
    };

    assert!(batch.has_more);
    assert!(batch.server_revision > batch.batch_revision);
    assert_eq!(confirm.target_revision, 12);
}

#[test]
fn sync_channel_serializes_with_stable_names() {
    assert_eq!(
        to_test_json(&SyncChannel::Directory),
        "\"directory\"".to_string()
    );
    assert_eq!(
        to_test_json(&SyncChannel::Credential),
        "\"credential\"".to_string()
    );
}

fn to_test_json<T: Serialize>(value: &T) -> String {
    value.serialize(TestJsonSerializer).unwrap()
}

struct TestJsonSerializer;

#[derive(Debug)]
struct TestJsonError(&'static str);

impl fmt::Display for TestJsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for TestJsonError {}

impl ser::Error for TestJsonError {
    fn custom<T: fmt::Display>(_message: T) -> Self {
        Self("unsupported serialization")
    }
}

impl Serializer for TestJsonSerializer {
    type Ok = String;
    type Error = TestJsonError;
    type SerializeSeq = TestJsonSeq;
    type SerializeTuple = Impossible<String, TestJsonError>;
    type SerializeTupleStruct = Impossible<String, TestJsonError>;
    type SerializeTupleVariant = Impossible<String, TestJsonError>;
    type SerializeMap = Impossible<String, TestJsonError>;
    type SerializeStruct = TestJsonStruct;
    type SerializeStructVariant = Impossible<String, TestJsonError>;

    fn serialize_bool(self, _value: bool) -> Result<Self::Ok, Self::Error> {
        Ok(_value.to_string())
    }

    fn serialize_i8(self, _value: i8) -> Result<Self::Ok, Self::Error> {
        Err(TestJsonError("unsupported serialization"))
    }

    fn serialize_i16(self, _value: i16) -> Result<Self::Ok, Self::Error> {
        Err(TestJsonError("unsupported serialization"))
    }

    fn serialize_i32(self, _value: i32) -> Result<Self::Ok, Self::Error> {
        Err(TestJsonError("unsupported serialization"))
    }

    fn serialize_i64(self, _value: i64) -> Result<Self::Ok, Self::Error> {
        Err(TestJsonError("unsupported serialization"))
    }

    fn serialize_u8(self, _value: u8) -> Result<Self::Ok, Self::Error> {
        Ok(_value.to_string())
    }

    fn serialize_u16(self, _value: u16) -> Result<Self::Ok, Self::Error> {
        Ok(_value.to_string())
    }

    fn serialize_u32(self, _value: u32) -> Result<Self::Ok, Self::Error> {
        Ok(_value.to_string())
    }

    fn serialize_u64(self, _value: u64) -> Result<Self::Ok, Self::Error> {
        Ok(_value.to_string())
    }

    fn serialize_f32(self, _value: f32) -> Result<Self::Ok, Self::Error> {
        Err(TestJsonError("unsupported serialization"))
    }

    fn serialize_f64(self, _value: f64) -> Result<Self::Ok, Self::Error> {
        Err(TestJsonError("unsupported serialization"))
    }

    fn serialize_char(self, _value: char) -> Result<Self::Ok, Self::Error> {
        Ok(format!("\"{_value}\""))
    }

    fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
        Ok(format!(
            "\"{}\"",
            value.replace('\\', "\\\\").replace('"', "\\\"")
        ))
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<Self::Ok, Self::Error> {
        Err(TestJsonError("unsupported serialization"))
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Ok("null".to_string())
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<Self::Ok, Self::Error> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok("null".to_string())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        Ok("null".to_string())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        Err(TestJsonError("unsupported serialization"))
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        Err(TestJsonError("unsupported serialization"))
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(TestJsonSeq { values: Vec::new() })
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Err(TestJsonError("unsupported serialization"))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Err(TestJsonError("unsupported serialization"))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Err(TestJsonError("unsupported serialization"))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Err(TestJsonError("unsupported serialization"))
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(TestJsonStruct { fields: Vec::new() })
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(TestJsonError("unsupported serialization"))
    }
}

struct TestJsonSeq {
    values: Vec<String>,
}

impl SerializeSeq for TestJsonSeq {
    type Ok = String;
    type Error = TestJsonError;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.values.push(value.serialize(TestJsonSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(format!("[{}]", self.values.join(",")))
    }
}

struct TestJsonStruct {
    fields: Vec<(&'static str, String)>,
}

impl SerializeStruct for TestJsonStruct {
    type Ok = String;
    type Error = TestJsonError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        self.fields
            .push((key, value.serialize(TestJsonSerializer)?));
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let fields = self
            .fields
            .into_iter()
            .map(|(key, value)| format!("\"{key}\":{value}"))
            .collect::<Vec<_>>()
            .join(",");
        Ok(format!("{{{fields}}}"))
    }
}
