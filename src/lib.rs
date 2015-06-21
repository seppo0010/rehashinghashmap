use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;
use std::iter::Chain;
use std::collections::hash_map;

pub struct RehashingHashMap<K, V> {
    // NOTE: I tried to make an array of 2 elements, but run into borrowing problems
    hashmap1: HashMap<K, V>,
    hashmap2: HashMap<K, V>,
    is1main: bool,
    rehashing: bool,
}

impl<K, V> RehashingHashMap<K, V>
    where K: Eq + Hash + Clone
{
    pub fn new() -> RehashingHashMap<K, V> {
        RehashingHashMap {
            hashmap1: HashMap::new(),
            hashmap2: HashMap::new(),
            is1main: true,
            rehashing: false,
        }
    }

    pub fn with_capacity(capacity: usize) -> RehashingHashMap<K, V> {
        RehashingHashMap {
            hashmap1: HashMap::with_capacity(capacity),
            hashmap2: HashMap::new(),
            is1main: true,
            rehashing: false,
        }
    }

    fn get_main(&self) -> &HashMap<K, V> {
        if self.is1main { &self.hashmap1 } else { &self.hashmap2 }
    }

    fn get_mut_main(&mut self) -> &mut HashMap<K, V> {
        if self.is1main { &mut self.hashmap1 } else { &mut self.hashmap2 }
    }

    fn get_secondary(&self) -> &HashMap<K, V> {
        if self.is1main { &self.hashmap2 } else { &self.hashmap1 }
    }

    fn get_mut_secondary(&mut self) -> &mut HashMap<K, V> {
        if self.is1main { &mut self.hashmap2 } else { &mut self.hashmap1 }
    }

    pub fn rehash(&mut self) {
        if self.rehashing {
            if self.get_secondary().len() == 0 {
                self.drop_secondary();
                return;
            }
            let (mut main, mut sec) = if self.is1main {
                (&mut self.hashmap1, &mut self.hashmap2)
            } else {
                (&mut self.hashmap2, &mut self.hashmap1)
            };
            // unwrap is safe, checked len() > 0 already
            let k: K = sec.keys().take(1).next().unwrap().clone();
            // FIXME: I wish I did not have to clone they key
            // unwrap is safe, we know the key exists in the hashmap
            let val = sec.remove(&k).unwrap();
            main.insert(k, val);
        }
    }

    pub fn capacity(&self) -> usize {
        self.get_main().capacity()
    }

    pub fn reserve(&mut self, additional: usize) {
        self.rehash();
        self.get_mut_main().reserve(additional)
    }

    pub fn is_rehashing(&self) -> bool {
        if !self.rehashing {
            assert_eq!(self.get_secondary().len(), 0);
        }
        self.rehashing
    }

    pub fn shrink_to_fit(&mut self) {
        if !self.rehashing {
            self.rehashing = true;
            self.is1main = !self.is1main;
            let len = self.len();
            self.get_mut_main().reserve(len)
        }
    }

    pub fn len(&self) -> usize {
        self.get_main().len() + self.get_secondary().len()
    }

    pub fn is_empty(&self) -> bool {
        self.get_main().is_empty() && self.get_secondary().is_empty()
    }

    fn drop_secondary(&mut self) {
        self.rehashing = false;
        assert_eq!(self.get_secondary().len(), 0);
        if self.is1main {
            self.hashmap2 = HashMap::new();
        } else {
            self.hashmap1 = HashMap::new();
        }
    }

    fn assert_state(&self) {
        #![allow(dead_code)]
        if self.rehashing {
            assert!(self.get_secondary().capacity() > 0);
        } else {
            assert!(self.get_secondary().capacity() == 0);
        }
    }

    pub fn clear(&mut self) {
        self.get_mut_main().clear();
        self.drop_secondary();
    }

    pub fn insert(&mut self, k: K, v: V) -> Option<V> {
        // while rehashing, they key can be in either hashmap1 or hashmap2
        // but we want to remove them from wherever it is and add it to main
        let mut ret = None;
        if self.rehashing || self.is1main {
            ret = self.hashmap1.remove(&k);
        }
        if ret.is_none() && (self.rehashing || !self.is1main) {
            ret = self.hashmap2.remove(&k);
        }
        self.get_mut_main().insert(k, v);
        self.rehash();
        ret
    }

    pub fn get<Q: ?Sized>(&self, k: &Q) -> Option<&V>
            where K: Borrow<Q>, Q: Hash + Eq {
        if self.rehashing {
            match self.get_main().get(k) {
                Some(ref v) => Some(v),
                None => self.get_secondary().get(k),
            }
        } else {
            self.get_main().get(k)
        }
    }

    pub fn remove<Q: ?Sized>(&mut self, k: &Q) -> Option<V>
        where K: Borrow<Q>, Q: Hash + Eq {
        if self.rehashing {
            self.rehash();
            match self.get_mut_main().remove(k) {
                Some(v) => Some(v),
                None => self.get_mut_secondary().remove(k),
            }
        } else {
            self.get_mut_main().remove(k)
        }
    }

    pub fn iter(&self) -> Iter<K, V> {
        Iter {
            inner: self.hashmap1.iter().chain(self.hashmap2.iter()),
            len: self.hashmap1.len() + self.hashmap2.len(),
        }
    }

    pub fn keys(&self) -> Keys<K, V> {
        Keys {
            inner: self.hashmap1.keys().chain(self.hashmap2.keys()),
            len: self.hashmap1.len() + self.hashmap2.len(),
        }
    }

    pub fn values(&self) -> Values<K, V> {
        Values {
            inner: self.hashmap1.values().chain(self.hashmap2.values()),
            len: self.hashmap1.len() + self.hashmap2.len(),
        }
    }
}

#[derive(Clone)]
pub struct Iter<'a, K: 'a, V: 'a> {
    inner: Chain<hash_map::Iter<'a, K, V>, hash_map::Iter<'a, K, V>>,
    len: usize,
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    #[inline] fn next(&mut self) -> Option<(&'a K, &'a V)> { self.inner.next() }
    #[inline] fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}

impl<'a, K, V> ExactSizeIterator for Iter<'a, K, V> {
    #[inline] fn len(&self) -> usize { self.len }
}

#[derive(Clone)]
pub struct Keys<'a, K: 'a, V: 'a> {
    inner: Chain<hash_map::Keys<'a, K, V>, hash_map::Keys<'a, K, V>>,
    len: usize,
}

impl<'a, K, V> Iterator for Keys<'a, K, V> {
    type Item = &'a K;

    #[inline] fn next(&mut self) -> Option<&'a K> { self.inner.next() }
    #[inline] fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}

impl<'a, K, V> ExactSizeIterator for Keys<'a, K, V> {
    #[inline] fn len(&self) -> usize { self.len }
}

#[derive(Clone)]
pub struct Values<'a, K: 'a, V: 'a> {
    inner: Chain<hash_map::Values<'a, K, V>, hash_map::Values<'a, K, V>>,
    len: usize,
}

impl<'a, K, V> Iterator for Values<'a, K, V> {
    type Item = &'a V;

    #[inline] fn next(&mut self) -> Option<&'a V> { self.inner.next() }
    #[inline] fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}

impl<'a, K, V> ExactSizeIterator for Values<'a, K, V> {
    #[inline] fn len(&self) -> usize { self.len }
}

#[test]
fn capacity() {
    let mut hash:RehashingHashMap<u8, u8> = RehashingHashMap::with_capacity(20);
    assert!(hash.capacity() >= 20);
    hash.reserve(40);
    assert!(hash.capacity() >= 40);
}

#[test]
fn insert() {
    let mut hash = RehashingHashMap::new();
    let key = 0;
    let value1 = 2;
    let value2 = 3;

    assert_eq!(hash.insert(key.clone(), value1.clone()), None);
    assert_eq!(hash.insert(key.clone(), value2.clone()), Some(value1.clone()));
    hash.shrink_to_fit();
    assert!(hash.is_rehashing());
    assert_eq!(hash.insert(key.clone(), value1.clone()), Some(value2.clone()));
    assert!(!hash.is_rehashing());
    hash.assert_state();
}

#[test]
fn insert_many_rehash_get() {
    let mut hash = RehashingHashMap::new();

    let len = 1000;

    for i in 0..len {
        hash.insert(i.clone(), i.clone());
    }
    hash.shrink_to_fit();
    for _ in 0..(len / 2){
        hash.rehash();
    }
    assert!(hash.is_rehashing());

    assert_eq!(hash.len(), len);

    for i in 0..len {
        assert_eq!(hash.get(&i).unwrap(), &i);
    }
    for i in len..(len * 2) {
        assert!(hash.get(&i).is_none());
    }

    for _ in 0..(len / 2 + 1){
        hash.rehash();
    }
    assert!(!hash.is_rehashing());
    hash.assert_state();

    assert_eq!(hash.len(), len);

    for i in 0..len {
        assert_eq!(hash.get(&i).unwrap(), &i);
    }
    for i in len..(len * 2) {
        assert!(hash.get(&i).is_none());
    }
}

#[test]
fn is_empty() {
    let mut hash = RehashingHashMap::new();
    assert!(hash.is_empty());

    let key = 0;
    let value = 2;
    assert_eq!(hash.insert(key.clone(), value.clone()), None);
    assert!(!hash.is_empty());
    hash.shrink_to_fit();
    assert!(hash.is_rehashing());
    assert!(!hash.is_empty());
    hash.rehash();
    hash.rehash();
    assert!(!hash.is_rehashing());
    assert!(!hash.is_empty());
}

#[test]
fn clear() {
    let mut hash = RehashingHashMap::with_capacity(1000);
    let key = 0;
    let value = 2;
    assert_eq!(hash.insert(key.clone(), value.clone()), None);
    hash.clear();
    hash.assert_state();

    assert!(hash.capacity() >= 1000);
}

#[test]
fn remove0() {
    let mut hash = RehashingHashMap::new();
    let key = 0;
    let value = 2;
    assert_eq!(hash.insert(key.clone(), value.clone()), None);
    hash.shrink_to_fit();
    assert!(hash.is_rehashing());
    assert_eq!(hash.remove(&key).unwrap(), value);
}

#[test]
fn remove1() {
    let mut hash = RehashingHashMap::new();
    let key = 0;
    let value = 2;
    assert_eq!(hash.insert(key.clone(), value.clone()), None);
    hash.shrink_to_fit();
    hash.rehash();
    assert!(hash.is_rehashing());
    assert_eq!(hash.remove(&key).unwrap(), value);
}

#[test]
fn remove2() {
    let mut hash = RehashingHashMap::new();
    let key = 0;
    let value = 2;
    assert_eq!(hash.insert(key.clone(), value.clone()), None);
    hash.shrink_to_fit();
    hash.rehash();
    hash.rehash();
    assert!(!hash.is_rehashing());
    assert_eq!(hash.remove(&key).unwrap(), value);
}

#[test]
fn iterator() {
    let len = 100;
    let mut hash = RehashingHashMap::with_capacity(len);
    let mut control = HashMap::new();
    for i in 0..len {
        hash.insert(i.clone(), i.clone());
        control.insert(i.clone(), i.clone());
    }
    hash.shrink_to_fit();
    for _ in 0..(len / 2) {
        hash.rehash();
    }
    assert!(hash.is_rehashing());

    assert_eq!(hash.iter().len(), len);
    for (_, i) in hash.iter() {
        control.remove(&i);
    }
    assert!(control.is_empty());
}

#[test]
fn keys() {
    let len = 100;
    let mut hash = RehashingHashMap::with_capacity(len);
    let mut control = HashMap::new();
    for i in 0..len {
        hash.insert(i.clone(), i.clone());
        control.insert(i.clone(), i.clone());
    }
    hash.shrink_to_fit();
    for _ in 0..(len / 2) {
        hash.rehash();
    }
    assert!(hash.is_rehashing());

    assert_eq!(hash.keys().len(), len);
    for i in hash.keys() {
        control.remove(&i);
    }
    assert!(control.is_empty());
}

#[test]
fn values() {
    let len = 100;
    let mut hash = RehashingHashMap::with_capacity(len);
    let mut control = HashMap::new();
    for i in 0..len {
        hash.insert(i.clone(), i.clone());
        control.insert(i.clone(), i.clone());
    }
    hash.shrink_to_fit();
    for _ in 0..(len / 2) {
        hash.rehash();
    }
    assert!(hash.is_rehashing());

    assert_eq!(hash.values().len(), len);
    for i in hash.values() {
        control.remove(&i);
    }
    assert!(control.is_empty());
}
