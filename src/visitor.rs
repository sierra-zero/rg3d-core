use std::{
    rc::{Rc},
    collections::HashMap,
    fs::File,
    any::Any,
    path::{Path, PathBuf},
    cell::{RefCell, Cell},
    io::{Write, Read, BufReader, BufWriter},
    string::FromUtf8Error,
    fmt::{Display, Formatter},
    sync::{Arc, Mutex},
    hash::Hash,
    collections::hash_map::Entry
};
use byteorder::{
    ReadBytesExt,
    WriteBytesExt,
    LittleEndian,
};
use crate::{
    math::{
        vec3::Vec3,
        quat::Quat,
        mat4::Mat4,
    },
    pool::{Handle, Pool},
};
use crate::math::mat3::Mat3;

pub enum FieldKind {
    Bool(bool),
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    U64(u64),
    I64(i64),
    F32(f32),
    F64(f64),
    Vec3(Vec3),
    Quat(Quat),
    Mat4(Mat4),
    Data(Vec<u8>),
    Mat3(Mat3)
}

impl FieldKind {
    fn as_string(&self) -> String {
        match self {
            FieldKind::Bool(data) => format!("<bool = {}>, ", data),
            FieldKind::U8(data) => format!("<u8 = {}>, ", data),
            FieldKind::I8(data) => format!("<i8 = {}>, ", data),
            FieldKind::U16(data) => format!("<u16 = {}>, ", data),
            FieldKind::I16(data) => format!("<i16 = {}>, ", data),
            FieldKind::U32(data) => format!("<u32 = {}>, ", data),
            FieldKind::I32(data) => format!("<i32 = {}>, ", data),
            FieldKind::U64(data) => format!("<u64 = {}>, ", data),
            FieldKind::I64(data) => format!("<i64 = {}>, ", data),
            FieldKind::F32(data) => format!("<f32 = {}>, ", data),
            FieldKind::F64(data) => format!("<f64 = {}>, ", data),
            FieldKind::Vec3(data) => {
                format!("<vec3 = {}; {}; {}>, ", data.x, data.y, data.z)
            }
            FieldKind::Quat(data) => {
                format!("<quat = {}; {}; {}; {}>, ", data.x, data.y, data.z, data.w)
            }
            FieldKind::Mat4(data) => {
                let mut out = String::from("<mat4 = ");
                for f in &data.f {
                    out += format!("{}; ", f).as_str();
                }
                out
            }
            FieldKind::Data(data) => {
                let out = match String::from_utf8(data.clone()) {
                    Ok(s) => s,
                    Err(_) => base64::encode(data)
                };
                format!("<data = {}>, ", out)
            }
            FieldKind::Mat3(data) => {
                let mut out = String::from("<mat3 = ");
                for f in &data.f {
                    out += format!("{}; ", f).as_str();
                }
                out
            }
        }
    }
}

pub trait FieldData {
    fn read(&mut self, kind: &FieldKind) -> VisitResult;
    fn write(&self) -> FieldKind;
}

macro_rules! impl_field_data (($type_name:ty, $($kind:tt)*) => {
    impl FieldData for $type_name {
        fn read(& mut self, kind: &FieldKind) -> VisitResult {
            match kind {
                $($kind)*(data) => {
                    *self = data.clone();
                    Ok(())
                },
                _ => Err(VisitError::FieldTypeDoesNotMatch)
            }
        }

        fn write(&self) -> FieldKind {
             $($kind)*(self.clone())
        }
    }
});

/// Proxy struct for plain data, we can't use Vec<u8> directly,
/// because it will serialize each byte as separate node.
pub struct Data<'a> {
    vec: &'a mut Vec<u8>
}

impl_field_data!(u64, FieldKind::U64);
impl_field_data!(i64, FieldKind::I64);
impl_field_data!(u32, FieldKind::U32);
impl_field_data!(i32, FieldKind::I32);
impl_field_data!(u16, FieldKind::U16);
impl_field_data!(i16, FieldKind::I16);
impl_field_data!(u8, FieldKind::U8);
impl_field_data!(i8, FieldKind::I8);
impl_field_data!(f32, FieldKind::F32);
impl_field_data!(f64, FieldKind::F64);
impl_field_data!(Vec3, FieldKind::Vec3);
impl_field_data!(Quat, FieldKind::Quat);
impl_field_data!(Mat4, FieldKind::Mat4);
impl_field_data!(bool, FieldKind::Bool);
impl_field_data!(Mat3, FieldKind::Mat3);

impl<T> Visit for T where T: FieldData + 'static {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        if visitor.reading {
            if let Some(field) = visitor.find_field(name) {
                self.read(&field.kind)
            } else {
                Err(VisitError::FieldDoesNotExist(name.to_owned()))
            }
        } else if visitor.find_field(name).is_some() {
            Err(VisitError::FieldAlreadyExists(name.to_owned()))
        } else {
            let node = visitor.current_node();
            node.fields.push(Field::new(name, self.write()));
            Ok(())
        }
    }
}

impl<'a> Visit for Data<'a> {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        if visitor.reading {
            if let Some(field) = visitor.find_field(name) {
                match &field.kind {
                    FieldKind::Data(data) => {
                        *self.vec = data.clone();
                        Ok(())
                    }
                    _ => Err(VisitError::FieldTypeDoesNotMatch)
                }
            } else {
                Err(VisitError::FieldDoesNotExist(name.to_owned()))
            }
        } else if visitor.find_field(name).is_some() {
            Err(VisitError::FieldAlreadyExists(name.to_owned()))
        } else {
            let node = visitor.current_node();
            node.fields.push(Field::new(name, FieldKind::Data(self.vec.clone())));
            Ok(())
        }
    }
}

pub struct Field {
    name: String,
    kind: FieldKind,
}

#[derive(Debug)]
pub enum VisitError {
    Io(std::io::Error),
    UnknownFieldType(u8),
    FieldDoesNotExist(String),
    FieldAlreadyExists(String),
    RegionAlreadyExists(String),
    InvalidCurrentNode,
    FieldTypeDoesNotMatch,
    RegionDoesNotExist(String),
    NoActiveNode,
    NotSupportedFormat,
    InvalidName,
    TypeMismatch,
    RefCellAlreadyMutableBorrowed,
    User(String),
    UnexpectedRcNullIndex,
    PoisonedMutex,
}

impl Display for VisitError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            VisitError::Io(io) => write!(f, "io error: {}", io),
            VisitError::UnknownFieldType(type_index) => write!(f, "unknown field type {}", type_index),
            VisitError::FieldDoesNotExist(name) => write!(f, "field does not exists {}", name),
            VisitError::FieldAlreadyExists(name) => write!(f, "field already exists {}", name),
            VisitError::RegionAlreadyExists(name) => write!(f, "region already exists {}", name),
            VisitError::InvalidCurrentNode => write!(f, "invalid current node"),
            VisitError::FieldTypeDoesNotMatch => write!(f, "field type does not match"),
            VisitError::RegionDoesNotExist(name) => write!(f, "region does not exists {}", name),
            VisitError::NoActiveNode => write!(f, "no active node"),
            VisitError::NotSupportedFormat => write!(f, "not supported format"),
            VisitError::InvalidName => write!(f, "invalid name"),
            VisitError::TypeMismatch => write!(f, "type mismatch"),
            VisitError::RefCellAlreadyMutableBorrowed => write!(f, "ref cell already mutable borrowed"),
            VisitError::User(msg) => write!(f, "user defined error: {}", msg),
            VisitError::UnexpectedRcNullIndex => write!(f, "unexpected rc null index"),
            VisitError::PoisonedMutex => write!(f, "attempt to lock poisoned mutex"),
        }
    }
}

impl<'a, T> From<std::sync::PoisonError<std::sync::MutexGuard<'a, T>>> for VisitError {
    fn from(_: std::sync::PoisonError<std::sync::MutexGuard<'a, T>>) -> Self {
        VisitError::PoisonedMutex
    }
}

impl From<std::io::Error> for VisitError {
    fn from(io_err: std::io::Error) -> Self {
        VisitError::Io(io_err)
    }
}

impl From<FromUtf8Error> for VisitError {
    fn from(_: FromUtf8Error) -> Self {
        VisitError::InvalidName
    }
}

impl From<String> for VisitError {
    fn from(s: String) -> Self {
        VisitError::User(s)
    }
}

pub type VisitResult = Result<(), VisitError>;

impl Field {
    pub fn new(name: &str, kind: FieldKind) -> Self {
        Self {
            name: name.to_owned(),
            kind,
        }
    }

    fn save(field: &Field, file: &mut dyn Write) -> VisitResult {
        let name = field.name.as_bytes();
        file.write_u32::<LittleEndian>(name.len() as u32)?;
        file.write_all(name)?;
        match &field.kind {
            FieldKind::U8(data) => {
                file.write_u8(1)?;
                file.write_u8(*data)?;
            }
            FieldKind::I8(data) => {
                file.write_i8(2)?;
                file.write_i8(*data)?;
            }
            FieldKind::U16(data) => {
                file.write_u8(3)?;
                file.write_u16::<LittleEndian>(*data)?;
            }
            FieldKind::I16(data) => {
                file.write_u8(4)?;
                file.write_i16::<LittleEndian>(*data)?;
            }
            FieldKind::U32(data) => {
                file.write_u8(5)?;
                file.write_u32::<LittleEndian>(*data)?;
            }
            FieldKind::I32(data) => {
                file.write_u8(6)?;
                file.write_i32::<LittleEndian>(*data)?;
            }
            FieldKind::U64(data) => {
                file.write_u8(7)?;
                file.write_u64::<LittleEndian>(*data)?;
            }
            FieldKind::I64(data) => {
                file.write_u8(8)?;
                file.write_i64::<LittleEndian>(*data)?;
            }
            FieldKind::F32(data) => {
                file.write_u8(9)?;
                file.write_f32::<LittleEndian>(*data)?;
            }
            FieldKind::F64(data) => {
                file.write_u8(10)?;
                file.write_f64::<LittleEndian>(*data)?;
            }
            FieldKind::Vec3(data) => {
                file.write_u8(11)?;
                file.write_f32::<LittleEndian>(data.x)?;
                file.write_f32::<LittleEndian>(data.y)?;
                file.write_f32::<LittleEndian>(data.z)?;
            }
            FieldKind::Quat(data) => {
                file.write_u8(12)?;
                file.write_f32::<LittleEndian>(data.x)?;
                file.write_f32::<LittleEndian>(data.y)?;
                file.write_f32::<LittleEndian>(data.z)?;
                file.write_f32::<LittleEndian>(data.w)?;
            }
            FieldKind::Mat4(data) => {
                file.write_u8(13)?;
                for f in &data.f {
                    file.write_f32::<LittleEndian>(*f)?;
                }
            }
            FieldKind::Data(data) => {
                file.write_u8(14)?;
                file.write_u32::<LittleEndian>(data.len() as u32)?;
                file.write_all(data.as_slice())?;
            }
            FieldKind::Bool(data) => {
                file.write_u8(15)?;
                file.write_u8(if *data { 1 } else { 0 })?;
            }
            FieldKind::Mat3(data) => {
                file.write_u8(16)?;
                for f in &data.f {
                    file.write_f32::<LittleEndian>(*f)?;
                }
            }
        }
        Ok(())
    }

    fn load(file: &mut dyn Read) -> Result<Field, VisitError> {
        let name_len = file.read_u32::<LittleEndian>()? as usize;
        let mut raw_name = Vec::with_capacity(name_len);
        unsafe { raw_name.set_len(name_len) };
        file.read_exact(raw_name.as_mut_slice())?;
        let id = file.read_u8()?;
        Ok(Field::new(String::from_utf8(raw_name)?.as_str(), match id {
            1 => FieldKind::U8(file.read_u8()?),
            2 => FieldKind::I8(file.read_i8()?),
            3 => FieldKind::U16(file.read_u16::<LittleEndian>()?),
            4 => FieldKind::I16(file.read_i16::<LittleEndian>()?),
            5 => FieldKind::U32(file.read_u32::<LittleEndian>()?),
            6 => FieldKind::I32(file.read_i32::<LittleEndian>()?),
            7 => FieldKind::U64(file.read_u64::<LittleEndian>()?),
            8 => FieldKind::I64(file.read_i64::<LittleEndian>()?),
            9 => FieldKind::F32(file.read_f32::<LittleEndian>()?),
            10 => FieldKind::F64(file.read_f64::<LittleEndian>()?),
            11 => FieldKind::Vec3({
                let x = file.read_f32::<LittleEndian>()?;
                let y = file.read_f32::<LittleEndian>()?;
                let z = file.read_f32::<LittleEndian>()?;
                Vec3 { x, y, z }
            }),
            12 => FieldKind::Quat({
                let x = file.read_f32::<LittleEndian>()?;
                let y = file.read_f32::<LittleEndian>()?;
                let z = file.read_f32::<LittleEndian>()?;
                let w = file.read_f32::<LittleEndian>()?;
                Quat { x, y, z, w }
            }),
            13 => FieldKind::Mat4({
                let mut f = [0.0f32; 16];
                for n in &mut f {
                    *n = file.read_f32::<LittleEndian>()?;
                }
                Mat4 { f }
            }),
            14 => FieldKind::Data({
                let len = file.read_u32::<LittleEndian>()? as usize;
                let mut vec = Vec::with_capacity(len);
                unsafe { vec.set_len(len) };
                file.read_exact(vec.as_mut_slice())?;
                vec
            }),
            15 => FieldKind::Bool(file.read_u8()? != 0),
            16 => FieldKind::Mat3({
                let mut f = [0.0f32; 9];
                for n in &mut f {
                    *n = file.read_f32::<LittleEndian>()?;
                }
                Mat3 { f }
            }),
            _ => return Err(VisitError::UnknownFieldType(id))
        }))
    }

    fn as_string(&self) -> String {
        format!("{}{}", self.name, self.kind.as_string())
    }
}

pub struct Node {
    name: String,
    fields: Vec<Field>,
    parent: Handle<Node>,
    children: Vec<Handle<Node>>,
}

impl Node {
    fn new(name: &str, parent: Handle<Node>) -> Self {
        Self {
            name: name.to_owned(),
            fields: Vec::new(),
            parent,
            children: Vec::new(),
        }
    }
}

impl Default for Node {
    fn default() -> Self {
        Self {
            name: String::new(),
            fields: Vec::new(),
            parent: Handle::NONE,
            children: Vec::new(),
        }
    }
}

pub struct Visitor {
    nodes: Pool<Node>,
    rc_map: HashMap<u64, Rc<dyn Any>>,
    arc_map: HashMap<u64, Arc<dyn Any + Send + Sync>>,
    reading: bool,
    current_node: Handle<Node>,
    root: Handle<Node>,
}

pub trait Visit {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult;
}

impl Default for Visitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Visitor {
    const MAGIC: &'static str = "RG3D";

    pub fn new() -> Self {
        let mut nodes = Pool::new();
        let root = nodes.spawn(Node::new("__ROOT__", Handle::NONE));
        Self {
            nodes,
            rc_map: HashMap::new(),
            arc_map: HashMap::new(),
            reading: false,
            current_node: root,
            root,
        }
    }

    fn find_field(&mut self, name: &str) -> Option<&mut Field> {
        let node = self.nodes.borrow_mut(self.current_node);
        for field in node.fields.iter_mut() {
            if field.name == name {
                return Some(field);
            }
        }
        None
    }

    pub fn is_reading(&self) -> bool {
        self.reading
    }

    fn current_node(&mut self) -> &mut Node {
        self.nodes.borrow_mut(self.current_node)
    }

    pub fn enter_region(&mut self, name: &str) -> VisitResult {
        if self.reading {
            let node = self.nodes.borrow(self.current_node);
            let mut region = Handle::NONE;
            for child_handle in node.children.iter() {
                let child = self.nodes.borrow(*child_handle);
                if child.name == name {
                    region = *child_handle;
                    break;
                }
            }
            if region.is_some() {
                self.current_node = region;
                Ok(())
            } else {
                Err(VisitError::RegionDoesNotExist(name.to_owned()))
            }
        } else {
            // Make sure that node does not exists already.
            let node = self.nodes.borrow(self.current_node);
            for child_handle in node.children.iter() {
                let child = self.nodes.borrow(*child_handle);
                if child.name == name {
                    return Err(VisitError::RegionAlreadyExists(name.to_owned()));
                }
            }

            let node_handle = self.nodes.spawn(Node::new(name, self.current_node));
            self.nodes.borrow_mut(self.current_node).children.push(node_handle);
            self.current_node = node_handle;

            Ok(())
        }
    }

    pub fn leave_region(&mut self) -> VisitResult {
        self.current_node = self.nodes.borrow(self.current_node).parent;
        if self.current_node.is_none() {
            Err(VisitError::NoActiveNode)
        } else {
            Ok(())
        }
    }

    fn print_node(&self, node_handle: Handle<Node>, nesting: usize, out_string: &mut String) {
        let offset = (0..nesting).map(|_| { "\t" }).collect::<String>();
        let node = self.nodes.borrow(node_handle);
        *out_string += format!("{}{}[Fields={}, Children={}]: ", offset, node.name, node.fields.len(), node.children.len()).as_str();
        for field in node.fields.iter() {
            *out_string += field.as_string().as_str();
        }

        *out_string += "\n";

        for child_handle in node.children.iter() {
            self.print_node(*child_handle, nesting + 1, out_string);
        }
    }

    pub fn save_text(&self) -> String {
        let mut out_string = String::new();
        self.print_node(self.root, 0, &mut out_string);
        out_string
    }

    pub fn save_binary(&self, path: &Path) -> VisitResult {
        let mut writer = BufWriter::new(File::create(path)?);
        writer.write_all(Self::MAGIC.as_bytes())?;
        let mut stack = Vec::new();
        stack.push(self.root);
        while let Some(node_handle) = stack.pop() {
            let node = self.nodes.borrow(node_handle);
            let name = node.name.as_bytes();
            writer.write_u32::<LittleEndian>(name.len() as u32)?;
            writer.write_all(name)?;

            writer.write_u32::<LittleEndian>(node.fields.len() as u32)?;
            for field in node.fields.iter() {
                Field::save(field, &mut writer)?
            }

            writer.write_u32::<LittleEndian>(node.children.len() as u32)?;
            for child_handle in node.children.iter() {
                stack.push(child_handle.clone());
            }
        }
        Ok(())
    }

    fn load_node_binary(&mut self, file: &mut dyn Read) -> Result<Handle<Node>, VisitError> {
        let name_len = file.read_u32::<LittleEndian>()? as usize;
        let mut raw_name = Vec::with_capacity(name_len);
        unsafe { raw_name.set_len(name_len) };
        file.read_exact(raw_name.as_mut_slice())?;

        let mut node = Node::default();
        node.name = String::from_utf8(raw_name)?;

        let field_count = file.read_u32::<LittleEndian>()? as usize;
        for _ in 0..field_count {
            let field = Field::load(file)?;
            node.fields.push(field);
        }

        let mut children = Vec::new();
        let child_count = file.read_u32::<LittleEndian>()? as usize;
        for _ in 0..child_count {
            children.push(self.load_node_binary(file)?);
        }

        node.children = children.clone();

        let handle = self.nodes.spawn(node);
        for child_handle in children.iter() {
            let child = self.nodes.borrow_mut(*child_handle);
            child.parent = handle;
        }

        Ok(handle)
    }

    pub fn load_binary(path: &Path) -> Result<Self, VisitError> {
        let mut reader = BufReader::new(File::open(path)?);
        let mut magic: [u8; 4] = Default::default();
        reader.read_exact(&mut magic)?;
        if !magic.eq(Self::MAGIC.as_bytes()) {
            return Err(VisitError::NotSupportedFormat);
        }
        let mut visitor = Self {
            nodes: Pool::new(),
            rc_map: Default::default(),
            arc_map: Default::default(),
            reading: true,
            current_node: Handle::NONE,
            root: Handle::NONE,
        };
        visitor.root = visitor.load_node_binary(&mut reader)?;
        visitor.current_node = visitor.root;
        Ok(visitor)
    }
}

impl<T> Visit for RefCell<T> where T: Visit + 'static {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        if let Ok(mut data) = self.try_borrow_mut() {
            data.visit(name, visitor)
        } else {
            Err(VisitError::RefCellAlreadyMutableBorrowed)
        }
    }
}

impl<T> Visit for Vec<T> where T: Default + Visit + 'static {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        visitor.enter_region(name)?;

        let mut len = self.len() as u32;
        len.visit("Length", visitor)?;

        if visitor.reading {
            for index in 0..len {
                let region_name = format!("Item{}", index);
                visitor.enter_region(region_name.as_str())?;
                let mut object = T::default();
                object.visit("ItemData", visitor)?;
                self.push(object);
                visitor.leave_region()?;
            }
        } else {
            for (index, item) in self.iter_mut().enumerate() {
                let region_name = format!("Item{}", index);
                visitor.enter_region(region_name.as_str())?;
                item.visit("ItemData", visitor)?;
                visitor.leave_region()?;
            }
        }
        visitor.leave_region()?;
        Ok(())
    }
}

impl<T> Visit for Option<T> where T: Default + Visit + 'static {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        visitor.enter_region(name)?;

        let mut is_some = if self.is_some() { 1u8 } else { 0u8 };
        is_some.visit("IsSome", visitor)?;

        if is_some != 0 {
            if visitor.reading {
                let mut value = T::default();
                value.visit("Data", visitor)?;
                *self = Some(value);
            } else {
                self.as_mut().unwrap().visit("Data", visitor)?;
            }
        }

        visitor.leave_region()?;
        Ok(())
    }
}

impl Visit for String {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        visitor.enter_region(name)?;

        let mut len = self.as_bytes().len() as u32;
        len.visit("Length", visitor)?;

        let mut data = if visitor.reading {
            Vec::new()
        } else {
            Vec::from(self.as_bytes())
        };

        let mut proxy = Data { vec: &mut data };
        proxy.visit("Data", visitor)?;

        if visitor.reading {
            *self = String::from_utf8(data)?;
        }
        visitor.leave_region()
    }
}

impl Visit for PathBuf {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        visitor.enter_region(name)?;

        let bytes = if let Some(path_str) = self.as_os_str().to_str() {
            path_str.as_bytes()
        } else {
            return Err(VisitError::InvalidName);
        };

        let mut len = bytes.len() as u32;
        len.visit("Length", visitor)?;

        let mut data = if visitor.reading {
            Vec::new()
        } else {
            Vec::from(bytes)
        };

        let mut proxy = Data { vec: &mut data };
        proxy.visit("Data", visitor)?;

        if visitor.reading {
            *self = PathBuf::from(String::from_utf8(data)?);
        }

        visitor.leave_region()
    }
}

impl<T> Visit for Cell<T> where T: Copy + Clone + Visit + 'static {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        let mut value = self.get();
        value.visit(name, visitor)?;
        if visitor.is_reading() {
            self.set(value);
        }
        Ok(())
    }
}

impl<T> Visit for Rc<T> where T: Default + Visit + 'static {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        visitor.enter_region(name)?;

        if visitor.reading {
            let mut raw = 0u64;
            raw.visit("Id", visitor)?;
            if raw == 0 {
                return Err(VisitError::UnexpectedRcNullIndex);
            }
            if let Some(ptr) = visitor.rc_map.get(&raw) {
                if let Ok(res) = Rc::downcast::<T>(ptr.clone()) {
                    *self = res;
                } else {
                    return Err(VisitError::TypeMismatch);
                }
            } else {
                // Remember that we already visited data Rc store.
                visitor.rc_map.insert(raw as u64, self.clone());

                let raw = rc_to_raw(self.clone());
                unsafe { &mut *raw }.visit("RcData", visitor)?;
            }
        } else {
            // Take raw pointer to inner data.
            let raw = rc_to_raw(self.clone());

            // Save it as id.
            let mut index = raw as u64;
            index.visit("Id", visitor)?;

            if let Entry::Vacant(entry) = visitor.rc_map.entry(index) {
                entry.insert(self.clone());
                unsafe { &mut *raw }.visit("RcData", visitor)?;
            }
        }

        visitor.leave_region()?;

        Ok(())
    }
}

impl<T> Visit for Mutex<T> where T: Default + Visit + Send {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        self.lock()?.visit(name, visitor)
    }
}

fn arc_to_raw<T>(arc: Arc<T>) -> *mut T {
    let raw = Arc::into_raw(arc) as *const T as *mut T;
    unsafe { Arc::from_raw(raw); };
    raw
}

fn rc_to_raw<T>(arc: Rc<T>) -> *mut T {
    let raw = Rc::into_raw(arc) as *const T as *mut T;
    unsafe { Rc::from_raw(raw); };
    raw
}

impl<T> Visit for Arc<T> where T: Default + Visit + Send + Sync + 'static {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        visitor.enter_region(name)?;

        if visitor.reading {
            let mut raw = 0u64;
            raw.visit("Id", visitor)?;
            if raw == 0 {
                return Err(VisitError::UnexpectedRcNullIndex);
            }
            if let Some(ptr) = visitor.arc_map.get(&raw) {
                if let Ok(res) = Arc::downcast::<T>(ptr.clone()) {
                    *self = res;
                } else {
                    return Err(VisitError::TypeMismatch);
                }
            } else {
                // Remember that we already visited data Rc store.
                visitor.arc_map.insert(raw as u64, self.clone());

                let raw = arc_to_raw(self.clone());
                unsafe { &mut *raw }.visit("ArcData", visitor)?;
            }
        } else {
            // Take raw pointer to inner data.
            let raw = arc_to_raw(self.clone());

            // Save it as id.
            let mut index = raw as u64;
            index.visit("Id", visitor)?;

            if let Entry::Vacant(entry) = visitor.arc_map.entry(index) {
                entry.insert(self.clone());
                unsafe { &mut *raw }.visit("ArcData", visitor)?;
            }
        }

        visitor.leave_region()?;

        Ok(())
    }
}

impl<T> Visit for std::rc::Weak<T> where T: Default + Visit + 'static {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        visitor.enter_region(name)?;

        if visitor.reading {
            let mut raw = 0u64;
            raw.visit("Id", visitor)?;

            if raw != 0 {
                if let Some(ptr) = visitor.rc_map.get(&raw) {
                    if let Ok(res) = Rc::downcast::<T>(ptr.clone()) {
                        *self = Rc::downgrade(&res);
                    } else {
                        return Err(VisitError::TypeMismatch);
                    }
                } else {
                    // Create new value wrapped into Rc and deserialize it.
                    let rc = Rc::new(T::default());
                    visitor.rc_map.insert(raw as u64, rc.clone());

                    let raw = rc_to_raw(rc.clone());
                    unsafe { &mut *raw }.visit("RcData", visitor)?;

                    *self = Rc::downgrade(&rc);
                }
            }
        } else if let Some(rc) = std::rc::Weak::upgrade(self) {
            // Take raw pointer to inner data.
            let raw = rc_to_raw(rc.clone());

            // Save it as id.
            let mut index = raw as u64;
            index.visit("Id", visitor)?;

            if let Entry::Vacant(entry) = visitor.rc_map.entry(index) {
                entry.insert(rc);
                unsafe { &mut *raw }.visit("RcData", visitor)?;
            }
        } else {
            let mut index = 0u64;
            index.visit("Id", visitor)?;
        }

        visitor.leave_region()?;

        Ok(())
    }
}

impl<T> Visit for std::sync::Weak<T> where T: Default + Visit + Send + Sync + 'static  {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        visitor.enter_region(name)?;

        if visitor.reading {
            let mut raw = 0u64;
            raw.visit("Id", visitor)?;

            if raw != 0 {
                if let Some(ptr) = visitor.arc_map.get(&raw) {
                    if let Ok(res) = Arc::downcast::<T>(ptr.clone()) {
                        *self = Arc::downgrade(&res);
                    } else {
                        return Err(VisitError::TypeMismatch);
                    }
                } else {
                    // Create new value wrapped into Arc and deserialize it.
                    let arc = Arc::new(T::default());
                    visitor.arc_map.insert(raw as u64, arc.clone());

                    let raw = arc_to_raw(arc.clone());
                    unsafe { &mut *raw }.visit("ArcData", visitor)?;

                    *self = Arc::downgrade(&arc);
                }
            }
        } else if let Some(arc) = std::sync::Weak::upgrade(self) {
            // Take raw pointer to inner data.
            let raw = arc_to_raw(arc.clone());

            // Save it as id.
            let mut index = raw as u64;
            index.visit("Id", visitor)?;

            if let Entry::Vacant(entry) = visitor.arc_map.entry(index) {
                entry.insert(arc);
                unsafe { &mut *raw }.visit("ArcData", visitor)?;
            }
        } else {
            let mut index = 0u64;
            index.visit("Id", visitor)?;
        }

        visitor.leave_region()?;

        Ok(())
    }
}

impl<K, V, S: std::hash::BuildHasher> Visit for HashMap<K, V, S> where K: Visit + Default + Clone + Hash + Eq, V: Visit + Default {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        visitor.enter_region(name)?;

        let mut count = self.len() as u32;
        count.visit("Count", visitor)?;

        if visitor.is_reading() {
            for i in 0..(count as usize) {
                let name = format!("Item{}", i);

                visitor.enter_region(name.as_str())?;

                let mut key = K::default();
                key.visit("Key", visitor)?;

                let mut value = V::default();
                value.visit("Value", visitor)?;

                self.insert(key, value);

                visitor.leave_region()?;
            }
        } else {
            for (i, (key, value)) in self.iter_mut().enumerate() {
                let name = format!("Item{}", i);

                visitor.enter_region(name.as_str())?;

                let mut key = key.clone();
                key.visit("Key", visitor)?;

                value.visit("Value", visitor)?;

                visitor.leave_region()?;
            }
        }

        visitor.leave_region()
    }
}

#[cfg(test)]
mod test {
    use std::{
        rc::Rc,
        path::Path,
        fs::File,
        io::Write,
    };
    use crate::visitor::{Visitor, Visit, VisitResult, VisitError, Data};

    pub struct Model {
        data: u64
    }

    pub struct Texture {
        data: Vec<u8>
    }

    impl Visit for Texture {
        fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
            visitor.enter_region(name)?;
            let mut proxy = Data { vec: &mut self.data };
            proxy.visit("Data", visitor)?;
            visitor.leave_region()
        }
    }

    impl Visit for Model {
        fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
            visitor.enter_region(name)?;
            self.data.visit("Data", visitor)?;
            visitor.leave_region()
        }
    }

    impl Visit for ResourceKind {
        fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
            match self {
                ResourceKind::Unknown => Err(VisitError::User(format!("invalid resource type"))),
                ResourceKind::Texture(tex) => tex.visit(name, visitor),
                ResourceKind::Model(model) => model.visit(name, visitor)
            }
        }
    }

    pub enum ResourceKind {
        Unknown,
        Model(Model),
        Texture(Texture),
    }

    struct Resource {
        kind: ResourceKind,
        data: u16,
    }

    impl Resource {
        fn new(kind: ResourceKind) -> Self {
            Self {
                kind,
                data: 0,
            }
        }
    }

    impl Default for Resource {
        fn default() -> Self {
            Self {
                kind: ResourceKind::Unknown,
                data: 0,
            }
        }
    }

    impl Visit for Resource {
        fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
            visitor.enter_region(name)?;
            if visitor.reading {} else {
                let mut kind_id: u8 = match &self.kind {
                    ResourceKind::Unknown => return Err(VisitError::User(format!("Invalid resource!"))),
                    ResourceKind::Model(_) => 0,
                    ResourceKind::Texture(_) => 1
                };
                kind_id.visit("KindId", visitor)?;
                self.kind.visit("KindData", visitor)?;
            }
            self.data.visit("ResData", visitor)?;
            visitor.leave_region()
        }
    }

    struct Foo {
        bar: u64,
        shared_resource: Option<Rc<Resource>>,
    }

    impl Default for Foo {
        fn default() -> Self {
            Self {
                bar: 0,
                shared_resource: None,
            }
        }
    }

    impl Foo {
        fn new(resource: Rc<Resource>) -> Self {
            Self {
                bar: 123,
                shared_resource: Some(resource),
            }
        }
    }

    impl Visit for Foo {
        fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
            visitor.enter_region(name)?;
            self.bar.visit("Bar", visitor)?;
            self.shared_resource.visit("SharedResource", visitor)?;
            visitor.leave_region()
        }
    }

    #[test]
    fn visitor_test() {
        let path = Path::new("test.bin");

        // Save
        {
            let mut visitor = Visitor::new();
            let mut resource = Rc::new(Resource::new(ResourceKind::Model(Model { data: 555 })));
            resource.visit("SharedResource", &mut visitor).unwrap();

            let mut objects = vec![
                Foo::new(resource.clone()),
                Foo::new(resource)
            ];

            objects.visit("Objects", &mut visitor).unwrap();

            visitor.save_binary(path).unwrap();
            if let Ok(mut file) = File::create(Path::new("test.txt")) {
                file.write(visitor.save_text().as_bytes()).unwrap();
            }
        }

        // Load
        {
            let mut visitor = Visitor::load_binary(path).unwrap();
            let mut resource: Rc<Resource> = Rc::new(Default::default());
            resource.visit("SharedResource", &mut visitor).unwrap();

            let mut objects: Vec<Foo> = Vec::new();
            objects.visit("Objects", &mut visitor).unwrap();
        }
    }
}