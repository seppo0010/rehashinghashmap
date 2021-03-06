use std::borrow::Borrow;
use std::collections::hash_map;
use std::collections::HashMap;
use std::hash::Hash;
use std::iter::Chain;
use std::iter::FromIterator;
use std::ops::Index;
use std::mem;
use std::sync::mpsc::channel;
use std::thread;

#[derive(Debug, Default)]
pub struct RehashingHashMap<K: Eq + Hash, V> {
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
        self.get_main().capacity() + self.get_secondary().len()
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
        let h = if self.is1main {
            mem::replace(&mut self.hashmap2, HashMap::new());
        } else {
            mem::replace(&mut self.hashmap1, HashMap::new());
        };
        let (tx, rx) = channel();
        thread::spawn(move || drop(rx.recv().unwrap()));
        tx.send(h).unwrap();
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

    pub fn get_mut<Q: ?Sized>(&mut self, k: &Q) -> Option<&mut V>
            where K: Borrow<Q>, Q: Hash + Eq {
        if self.rehashing {
            self.rehash();
            if self.get_main().contains_key(k) {
                self.get_mut_main().get_mut(k)
            } else {
                self.get_mut_secondary().get_mut(k)
            }
        } else {
            self.get_mut_main().get_mut(k)
        }
    }

    pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> bool
            where K: Borrow<Q>, Q: Hash + Eq {
        self.get_main().contains_key(k) || self.get_secondary().contains_key(k)
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

    pub fn entry(&mut self, key: K) -> hash_map::Entry<K, V> {
        self.rehash();
        if self.rehashing {
            if self.get_secondary().contains_key(&key) {
                return self.get_mut_secondary().entry(key);
            }
        }
        self.get_mut_main().entry(key)
    }

    pub fn iter(&self) -> Iter<K, V> {
        Iter {
            inner: self.hashmap1.iter().chain(self.hashmap2.iter()),
            len: self.hashmap1.len() + self.hashmap2.len(),
        }
    }

    pub fn iter_mut(&mut self) -> IterMut<K, V> {
        self.rehash();
        let len = self.hashmap1.len() + self.hashmap2.len();
        IterMut {
            inner: self.hashmap1.iter_mut().chain(self.hashmap2.iter_mut()),
            len: len,
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

impl<K, V> PartialEq for RehashingHashMap<K, V> where K: Eq + Hash + Clone, V: PartialEq {
    fn eq(&self, other: &RehashingHashMap<K, V>) -> bool {
        // we cannot rehash because `self` and `other` are not immutables!
        // so we should try to see if they are the same manually if they are
        // rehashing
        if !self.is_rehashing() && !other.is_rehashing() {
            return self.get_main().eq(other.get_main());
        }

        if self.len() != other.len() {
            return false;
        }

        for (k, v) in self.iter() {
            if other.get(k) != Some(v) {
                return false;
            }
        }
        return true;
    }
}

impl<'a, K, Q: ?Sized, V> Index<&'a Q> for RehashingHashMap<K, V>
    where K: Eq + Hash + Clone + Borrow<Q>,
    Q: Eq + Hash,
{
    type Output = V;

    #[inline]
    fn index(&self, index: &Q) -> &V {
        self.get(index).expect("no entry found for key")
    }
}

impl<'a, K, V> IntoIterator for &'a RehashingHashMap<K, V>
    where K: Eq + Hash + Clone
{
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Iter<'a, K, V> {
        self.iter()
    }
}

impl<'a, K, V> IntoIterator for &'a mut RehashingHashMap<K, V>
    where K: Eq + Hash + Clone
{
    type Item = (&'a K, &'a mut V);
    type IntoIter = IterMut<'a, K, V>;

    fn into_iter(mut self) -> IterMut<'a, K, V> {
        self.iter_mut()
    }
}

impl<K, V> FromIterator<(K, V)> for RehashingHashMap<K, V>
    where K: Eq + Hash + Clone
{
    fn from_iter<T: IntoIterator<Item=(K, V)>>(iterable: T) -> RehashingHashMap<K, V> {
        let iter = iterable.into_iter();
        let lower = iter.size_hint().0;
        let mut map = RehashingHashMap::with_capacity(lower);
        map.extend(iter);
        map
    }
}

impl<K, V> Extend<(K, V)> for RehashingHashMap<K, V>
    where K: Eq + Hash + Clone
{
    fn extend<T: IntoIterator<Item=(K, V)>>(&mut self, iter: T) {
        for (k, v) in iter {
            self.insert(k, v);
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

pub struct IterMut<'a, K: 'a, V: 'a> {
    inner: Chain<hash_map::IterMut<'a, K, V>, hash_map::IterMut<'a, K, V>>,
    len: usize,
}

impl<'a, K, V> Iterator for IterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);

    #[inline] fn next(&mut self) -> Option<(&'a K, &'a mut V)> { self.inner.next() }
    #[inline] fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}

impl<'a, K, V> ExactSizeIterator for IterMut<'a, K, V> {
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
fn iter_mut() {
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

    assert_eq!(hash.iter_mut().len(), len);
    for (_, i) in hash.iter_mut() {
        control.remove(&i);
        *i *= 2;
    }
    assert!(control.is_empty());

    // make sure mutability was saved
    for i in 0..len {
        assert_eq!(hash.get(&i).unwrap().clone(), i * 2);
    }
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

#[test]
fn entry() {
    let len = 100;
    let mut hash = RehashingHashMap::with_capacity(len);
    for i in 0..len {
        hash.insert(i.clone(), i.clone());
    }

    // modifying main
    {
        let v = hash.entry(0).or_insert(100); // updating
        *v += 1;
    }
    hash.entry(len).or_insert(len); // inserting

    hash.shrink_to_fit();
    // modifying secondary
    assert!(hash.is_rehashing());
    {
        let v = hash.entry(1).or_insert(100); // updating
        *v += 1;
    }
    hash.entry(len + 1).or_insert(len + 1); // inserting

    while hash.is_rehashing() {
        hash.rehash();
    }

    // modifying the new main
    {
        let v = hash.entry(2).or_insert(100); // updating
        *v += 1;
    }
    hash.entry(len + 2).or_insert(len + 2); // inserting

    for i in 0..(len + 3) {
        assert_eq!(hash.get(&i).unwrap().clone(), if i <= 2 { i + 1 } else { i });
    }
}

#[test]
fn contains_key() {
    let len = 100;
    let mut hash = RehashingHashMap::with_capacity(len);
    for i in 0..len {
        hash.insert(i.clone(), i.clone());
    }
    hash.shrink_to_fit();
    for _ in 0..(len / 2) {
        hash.rehash();
    }
    assert!(hash.is_rehashing());

    for i in 0..len {
        assert!(hash.contains_key(&i));
    }
    assert!(!hash.contains_key(&(len + 1)));
}

#[test]
fn get_mut0() {
    let mut hash = RehashingHashMap::new();
    let value = 1;
    {
        hash.insert(value.clone(), value.clone());
        hash.shrink_to_fit();
        assert!(hash.is_rehashing());
        let val = hash.get_mut(&value).unwrap();
        *val += 1;
    }
    assert_eq!(hash.get(&value).unwrap().clone(), 2);
}

#[test]
fn get_mut1() {
    let mut hash = RehashingHashMap::new();
    let value = 1;
    {
        hash.insert(value.clone(), value.clone());
        hash.shrink_to_fit();
        hash.rehash();
        assert!(hash.is_rehashing());
        let val = hash.get_mut(&value).unwrap();
        *val += 1;
    }
    assert_eq!(hash.get(&value).unwrap().clone(), 2);
}

#[test]
fn get_mut2() {
    let mut hash = RehashingHashMap::new();
    let value = 1;
    {
        hash.insert(value.clone(), value.clone());
        hash.shrink_to_fit();
        hash.rehash();
        hash.rehash();
        assert!(!hash.is_rehashing());
        let val = hash.get_mut(&value).unwrap();
        *val += 1;
    }
    assert_eq!(hash.get(&value).unwrap().clone(), 2);
}

#[test]
fn eq() {
    let mut hash1 = RehashingHashMap::new();
    let mut hash2 = RehashingHashMap::new();

    for i in 0..100 {
        hash1.insert(i.clone(), i.clone());
        hash2.insert(i.clone(), i.clone());
    }
    hash1.shrink_to_fit();
    hash2.shrink_to_fit();
    while hash2.is_rehashing() {
        assert_eq!(hash1, hash2);
        hash2.rehash();
    }
    hash2.shrink_to_fit();
    hash2.insert(101, 101);
    assert!(hash1 != hash2);
}

#[test]
fn index() {
    let mut hash = RehashingHashMap::new();

    for i in 0..100 {
        hash.insert(i.clone(), i.clone());
    }
    hash.shrink_to_fit();
    for i in 0..100 {
        hash.rehash();
        assert_eq!(hash[&i], i);
    }
}

#[test]
fn into_iter() {
    let len = 100;
    let mut hash = RehashingHashMap::new();
    let mut control = HashMap::new();
    for i in 0..len {
        hash.insert(i.clone(), i.clone());
        control.insert(i.clone(), i.clone());
    }
    hash.shrink_to_fit();
    for _ in 0..(len / 2) {
        hash.rehash();
    }

    for (k, v) in hash.into_iter() {
        assert_eq!(&control.remove(&k).unwrap(), v);
    }
    assert_eq!(control.len(), 0);
}

#[test]
fn extend() {
    let mut hash = RehashingHashMap::new();
    hash.extend(vec![(1, 1), (2, 2), (3, 3)]);
    assert_eq!(hash.len(), 3);
}

#[test]
fn from_iter() {
    let hash = RehashingHashMap::from_iter(vec![(1, 1), (2, 2), (3, 3)]);
    assert_eq!(hash.len(), 3);
}
