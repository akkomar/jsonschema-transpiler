/// A JSON Schema serde module derived from the v4 spec.
/// Refer to http://json-schema.org/draft-04/schema for spec details.
use serde_json::{json, Value};
use std::collections::HashMap;

use super::ast;

/// The type enumeration does not contain any data and is used to determine
/// available fields in the flattened tag. In JSONSchema parlance, these are
/// known as `simpleTypes`.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
enum Atom {
    Null,
    Boolean,
    Number,
    Integer,
    String,
    Object,
    Array,
}

enum Type {
    Atom(Atom),
    List(Vec<Atom>),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum AdditionalProperties {
    Bool(bool),
    Object(Box<Tag>),
}

#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct Object {
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<HashMap<String, Box<Tag>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    additional_properties: Option<AdditionalProperties>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pattern_properties: Option<Box<Tag>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<Vec<String>>,
}

/// Represent an array of subschemas. This is also known as a `schemaArray`.
type TagArray = Vec<Box<Tag>>;
type OneOf = TagArray;
type AllOf = TagArray;

#[derive(Serialize, Deserialize, Debug, Default)]
struct Array {
    // Using Option<TagArray> would support tuple validation
    #[serde(skip_serializing_if = "Option::is_none")]
    items: Option<Box<Tag>>,
}

/// Container for the main body of the schema.
#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase", tag = "type")]
pub struct Tag {
    #[serde(rename = "type", default)]
    data_type: Value,
    #[serde(flatten)]
    object: Object,
    #[serde(flatten)]
    array: Array,
    #[serde(skip_serializing_if = "Option::is_none")]
    one_of: Option<OneOf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    all_of: Option<AllOf>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

impl Tag {
    fn get_type(&self) -> Type {
        match &self.data_type {
            Value::String(string) => {
                let atom: Atom = serde_json::from_value(json!(string)).unwrap();
                Type::Atom(atom)
            }
            Value::Array(array) => {
                let list: Vec<Atom> = array
                    .iter()
                    .map(|v| serde_json::from_value(json!(v)).unwrap())
                    .collect();
                Type::List(list)
            }
            _ => Type::Atom(Atom::Object),
        }
    }

    pub fn type_into_ast(&self) -> ast::Tag {
        match self.get_type() {
            Type::Atom(atom) => self.atom_into_ast(atom),
            Type::List(list) => {
                let mut items: Vec<ast::Tag> = Vec::new();
                for atom in list {
                    items.push(self.atom_into_ast(atom));
                }
                ast::Tag::new(ast::Type::Union(ast::Union::new(items)), None, false)
            }
        }
    }

    fn atom_into_ast(&self, data_type: Atom) -> ast::Tag {
        match data_type {
            Atom::Null => ast::Tag::new(ast::Type::Null, None, false),
            Atom::Boolean => ast::Tag::new(ast::Type::Atom(ast::Atom::Boolean), None, false),
            Atom::Number => ast::Tag::new(ast::Type::Atom(ast::Atom::Number), None, false),
            Atom::Integer => ast::Tag::new(ast::Type::Atom(ast::Atom::Integer), None, false),
            Atom::String => ast::Tag::new(ast::Type::Atom(ast::Atom::String), None, false),
            Atom::Object => match &self.object.properties {
                Some(properties) => {
                    let mut fields: HashMap<String, ast::Tag> = HashMap::new();
                    for (key, value) in properties {
                        fields.insert(key.to_string(), value.type_into_ast());
                    }
                    ast::Tag::new(ast::Type::Object(ast::Object::new(fields)), None, false)
                }
                None => {
                    // handle maps
                    match (
                        &self.object.additional_properties,
                        &self.object.pattern_properties,
                    ) {
                        (Some(AdditionalProperties::Object(add)), Some(pat)) => {
                            let value = ast::Tag::new(
                                ast::Type::Union(ast::Union::new(vec![
                                    add.type_into_ast(),
                                    pat.type_into_ast(),
                                ])),
                                None,
                                false,
                            );

                            ast::Tag::new(ast::Type::Map(ast::Map::new(None, value)), None, false)
                        }
                        (Some(AdditionalProperties::Object(tag)), None) | (None, Some(tag)) => {
                            ast::Tag::new(
                                ast::Type::Map(ast::Map::new(None, tag.type_into_ast())),
                                None,
                                false,
                            )
                        }
                        _ => {
                            // handle oneOf
                            match &self.one_of {
                                Some(vec) => {
                                    let items =
                                        vec.iter().map(|item| item.type_into_ast()).collect();
                                    ast::Tag::new(
                                        ast::Type::Union(ast::Union::new(items)),
                                        None,
                                        false,
                                    )
                                }
                                None => {
                                    ast::Tag::new(ast::Type::Atom(ast::Atom::JSON), None, false)
                                }
                            }
                        }
                    }
                }
            },
            Atom::Array => {
                if let Some(items) = &self.array.items {
                    ast::Tag::new(
                        ast::Type::Array(ast::Array::new(items.type_into_ast())),
                        None,
                        false,
                    )
                } else {
                    panic!("array missing item")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_type_null() {
        let schema = Tag {
            data_type: json!("integer"),
            ..Default::default()
        };
        let expect = json!({
            "type": "integer"
        });
        assert_eq!(expect, json!(schema))
    }

    #[test]
    fn test_serialize_type_object_additional_properties_bool() {
        // check that the untagged attribute is working correctly
        let schema = Tag {
            data_type: json!("object"),
            object: Object {
                additional_properties: Some(AdditionalProperties::Bool(true)),
                ..Default::default()
            },
            ..Default::default()
        };
        let expect = json!({
            "type": "object",
            "additionalProperties": true,
        });
        assert_eq!(expect, json!(schema))
    }

    #[test]
    fn test_deserialize_type_null() {
        let data = json!({
            "type": "null"
        });
        let schema: Tag = serde_json::from_value(data).unwrap();
        assert_eq!(schema.data_type.as_str().unwrap(), "null");
        assert!(schema.extra.is_empty());
    }

    #[test]
    fn test_deserialize_type_object() {
        let data = json!({
            "type": "object",
            "properties": {
                "test-int": {"type": "integer"},
                "test-null": {"type": "null"}
            },
            "required": ["test-int"]
        });
        let schema: Tag = serde_json::from_value(data).unwrap();
        let props = schema.object.properties.unwrap();
        assert_eq!(props.len(), 2);
        let test_int = props.get("test-int").unwrap();
        assert_eq!(test_int.data_type, json!("integer"));
        assert_eq!(schema.object.required.unwrap(), vec!["test-int"]);
    }

    #[test]
    fn test_deserialize_type_object_additional_properties() {
        let data_true = json!({
            "type": "object",
            "additionalProperties": true
        });
        if let Ok(schema) = serde_json::from_value::<Tag>(data_true) {
            match schema.object.additional_properties {
                Some(AdditionalProperties::Bool(true)) => (),
                _ => panic!(),
            }
        } else {
            panic!()
        };
        let data_false = json!({
            "type": "object",
            "additionalProperties": false
        });
        if let Ok(schema) = serde_json::from_value::<Tag>(data_false) {
            match schema.object.additional_properties {
                Some(AdditionalProperties::Bool(false)) => (),
                _ => panic!(),
            }
        } else {
            panic!()
        };
        let data_object = json!({
            "type": "object",
            "additionalProperties": {"type": "integer"}
        });
        if let Ok(schema) = serde_json::from_value::<Tag>(data_object) {
            match schema.object.additional_properties {
                Some(AdditionalProperties::Object(object)) => {
                    assert_eq!(object.data_type, json!("integer"))
                }
                _ => panic!(),
            }
        } else {
            panic!()
        };
    }

    #[test]
    fn test_deserialize_type_nested_object() {
        let data = json!({
            "type": "object",
            "properties": {
                "nested-object": {
                    "type": "object",
                    "properties": {
                        "test-int": {"type": "int"}
                    }
                },
            }
        });
        let schema: Tag = serde_json::from_value(data).unwrap();
        let props = schema.object.properties.as_ref().unwrap();
        assert_eq!(props.len(), 1);
        let nested_object = *props.get("nested-object").as_ref().unwrap();
        assert_eq!(nested_object.data_type, json!("object"));
        let nested_object_props = nested_object.object.properties.as_ref().unwrap();
        assert_eq!(nested_object_props.len(), 1);
    }

    #[test]
    fn test_deserialize_type_array() {
        let data = json!({
            "type": "array",
            "items": {
                "type": "integer"
            }
        });
        let schema: Tag = serde_json::from_value(data).unwrap();
        let items = schema.array.items.unwrap();
        assert_eq!(items.data_type, json!("integer"))
    }

    #[test]
    fn test_deserialize_type_one_of() {
        let data = json!({
            "oneOf": [
                {"type": "integer"},
                {"type": "null"}
            ],
        });
        let schema: Tag = serde_json::from_value(data).unwrap();
        assert!(schema.data_type.is_null());
        let one_of = schema.one_of.unwrap();
        assert_eq!(one_of.len(), 2);
        assert_eq!(one_of[0].data_type, json!("integer"));
        assert_eq!(one_of[1].data_type, json!("null"));
    }

    #[test]
    fn test_deserialize_type_all_of() {
        let data = json!({
            "allOf": [
                {"type": "integer"},
                {"type": "null"}
            ],
        });
        let schema: Tag = serde_json::from_value(data).unwrap();
        assert!(schema.data_type.is_null());
        let all_of = schema.all_of.unwrap();
        assert_eq!(all_of.len(), 2);
        assert_eq!(all_of[0].data_type, json!("integer"));
        assert_eq!(all_of[1].data_type, json!("null"));
    }

    #[test]
    fn test_deserialize_extras() {
        let data = json!({
            "meta": "hello world!"
        });
        let schema: Tag = serde_json::from_value(data).unwrap();
        assert_eq!(schema.extra["meta"], json!("hello world!"))
    }

    #[test]
    fn test_into_ast_atom_null() {
        let data = json!({
            "type": "null"
        });
        let schema: Tag = serde_json::from_value(data).unwrap();
        let ast: ast::Tag = schema.into();
        let expect = json!({
            "type": "null",
            "nullable": false,
        });
        assert_eq!(expect, json!(ast))
    }

    #[test]
    fn test_into_ast_atom_integer() {
        let data = json!({
            "type": "integer"
        });
        let schema: Tag = serde_json::from_value(data).unwrap();
        let ast: ast::Tag = schema.into();
        let expect = json!({
            "type": {"atom": "integer"},
            "nullable": false,
        });
        assert_eq!(expect, json!(ast))
    }

    #[test]
    fn test_into_ast_list() {
        let data = json!({
            "type": ["null", "integer"]
        });
        let schema: Tag = serde_json::from_value(data).unwrap();
        let ast: ast::Tag = schema.into();
        let expect = json!({
            "type": {
                "union": {
                    "items": [
                        {"type": "null", "nullable": false},
                        {"type": {"atom": "integer"}, "nullable": false},
                    ]
                }
            },
            "nullable": false,
        });
        assert_eq!(expect, json!(ast))
    }

    #[test]
    fn test_into_ast_object() {
        // test using an atom and a nested object
        let data = json!({
        "type": "object",
        "properties": {
            "test-int": {"type": "integer"},
            "test-obj": {
                "type": "object",
                "properties": {
                    "test-null": {"type": "null"}
                }}}});
        let schema: Tag = serde_json::from_value(data).unwrap();
        let ast: ast::Tag = schema.into();
        let expect = json!({
        "nullable": false,
        "type": {
            "object": {
                "fields": {
                    "test-int": {
                        "name": "test-int",
                        "type": {"atom": "integer"},
                        "nullable": false,
                    },
                    "test-obj": {
                        "name": "test-obj",
                        "nullable": false,
                        "type": {
                            "object": {
                                "fields": {
                                    "test-null": {
                                        "name": "test-null",
                                        "type": "null",
                                        "nullable": false,
                                    }}}}}}}}});
        assert_eq!(expect, json!(ast))
    }

    #[test]
    fn test_into_ast_object_map() {
        let data = json!({
            "type": "object",
            "additionalProperties": {
                "type": "object",
                "properties": {
                    "test-int": {"type": "integer"}
                }
            }
        });
        let schema: Tag = serde_json::from_value(data).unwrap();
        let ast: ast::Tag = schema.into();
        let expect = json!({
        "nullable": false,
        "type": {
            "map": {
                "key": {
                    "name": "key",
                    "nullable": false,
                    "type": {"atom": "string"}
                },
                "value": {
                    "name": "value",
                    "nullable": false,
                    "type": {
                        "object": {
                            "fields": {
                                "test-int": {
                                    "name": "test-int",
                                    "nullable": false,
                                    "type": {"atom": "integer"}
                                }}}}}}}});
        assert_eq!(expect, json!(ast))
    }

    #[test]
    fn test_into_ast_array() {
        let data = json!({
            "type": "array",
            "items": {
                "type": "integer"
            }
        });
        let schema: Tag = serde_json::from_value(data).unwrap();
        let ast: ast::Tag = schema.into();
        let expect = json!({
        "nullable": false,
        "type": {
            "array": {
                "items": {
                    "nullable": false,
                    "type": {"atom": "integer"}
                }}}});
        assert_eq!(expect, json!(ast))
    }

    #[test]
    fn test_into_ast_one_of() {
        let data = json!({
            "oneOf": [
                {"type": "integer"},
                {"type": "null"}
            ],
        });
        let schema: Tag = serde_json::from_value(data).unwrap();
        let ast: ast::Tag = schema.into();
        let expect = json!({
        "nullable": false,
        "type": {
            "union": {
                "items": [
                    {
                        "nullable": false,
                        "type": {"atom": "integer"},
                    },
                    {
                        "nullable": false,
                        "type": "null"
                    }
                ]}}});
        assert_eq!(expect, json!(ast))
    }
}
