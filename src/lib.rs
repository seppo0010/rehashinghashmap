use std::hash::Hash;
use std::collections::HashMap;

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

    fn rehash(&mut self) {
        if self.rehashing {
            let (mut main, mut sec) = if self.is1main {
                (&mut self.hashmap1, &mut self.hashmap2)
            } else {
                (&mut self.hashmap2, &mut self.hashmap1)
            };
            let k: K = match sec.keys().take(1).next() {
                Some(k) => k.clone(),
                None => {
                    self.rehashing = false;
                    return;
                }
            };
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
}
