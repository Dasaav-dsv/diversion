use std::{fmt, iter};

use slab::Slab;

pub struct LinkedSlab<T> {
    inner: Slab<Node<T>>,
    first: usize,
}

struct Node<T> {
    item: T,
    next: usize,
    prev: usize,
}

impl<T> LinkedSlab<T> {
    pub const fn new() -> Self {
        Self {
            inner: Slab::new(),
            first: usize::MAX,
        }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    pub fn first(&self) -> Option<&T> {
        self.get(self.first)
    }

    pub fn first_mut(&mut self) -> Option<&mut T> {
        self.get_mut(self.first)
    }

    pub fn get(&self, key: usize) -> Option<&T> {
        let node = self.inner.get(key)?;
        Some(&node.item)
    }

    pub fn get_mut(&mut self, key: usize) -> Option<&mut T> {
        let node = self.inner.get_mut(key)?;
        Some(&mut node.item)
    }

    pub fn get_next(&self, key: usize) -> Option<&T> {
        let next = self.inner.get(key)?.next;
        self.get(next)
    }

    pub fn get_next_mut(&mut self, key: usize) -> Option<&mut T> {
        let next = self.inner.get(key)?.next;
        self.get_mut(next)
    }

    pub fn push_front(&mut self, item: T) -> usize {
        let old_first = self.first;

        self.first = self.inner.insert(Node {
            item,
            next: old_first,
            prev: usize::MAX,
        });

        if let Some(old_first) = self.inner.get_mut(old_first) {
            old_first.prev = self.first;
        }

        self.first
    }

    pub fn remove(&mut self, key: usize) -> Option<T> {
        let node = self.inner.try_remove(key)?;

        let prev = if let Some(prev) = self.inner.get_mut(node.prev) {
            prev.next = node.next;
            node.prev
        } else {
            self.first = node.next;
            usize::MAX
        };

        if let Some(next) = self.inner.get_mut(node.next) {
            next.prev = prev;
        }

        Some(node.item)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        let mut next = self.first;
        iter::from_fn(move || {
            let node = self.inner.get(next)?;
            next = node.next;
            Some(&node.item)
        })
    }
}

impl<T> Default for LinkedSlab<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> Clone for LinkedSlab<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            first: self.first,
        }
    }
}

impl<T: Clone> Clone for Node<T> {
    fn clone(&self) -> Self {
        Self {
            item: self.item.clone(),
            next: self.next,
            prev: self.prev,
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for LinkedSlab<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::linked_slab::LinkedSlab;

    #[test]
    fn get() {
        let mut list = LinkedSlab::new();

        let c = list.push_front('c');
        assert_eq!(list.get(c), Some(&'c'));

        let b = list.push_front('b');
        assert_eq!(list.get(b), Some(&'b'));
        assert_eq!(list.get(c), Some(&'c'));

        let a = list.push_front('a');
        assert_eq!(list.get(a), Some(&'a'));
        assert_eq!(list.get(b), Some(&'b'));
        assert_eq!(list.get(c), Some(&'c'));
    }

    #[test]
    fn get_next() {
        let mut list = LinkedSlab::new();

        let c = list.push_front('c');
        assert_eq!(list.get_next(c), None);

        let b = list.push_front('b');
        assert_eq!(list.get_next(b), Some(&'c'));
        assert_eq!(list.get_next(c), None);

        let a = list.push_front('a');
        assert_eq!(list.get_next(a), Some(&'b'));
        assert_eq!(list.get_next(b), Some(&'c'));
        assert_eq!(list.get_next(c), None);
    }

    #[test]
    fn remove() {
        let mut list = LinkedSlab::new();

        let c = list.push_front('c');
        assert_eq!(list.remove(c), Some('c'));

        let c = list.push_front('c');
        let b = list.push_front('b');
        assert_eq!(list.remove(b), Some('b'));
        assert_eq!(list.remove(c), Some('c'));

        let c = list.push_front('c');
        let b = list.push_front('b');
        let a = list.push_front('a');
        assert_eq!(list.remove(a), Some('a'));
        assert_eq!(list.remove(b), Some('b'));
        assert_eq!(list.remove(c), Some('c'));
    }

    #[test]
    fn rev_remove() {
        let mut list = LinkedSlab::new();

        let c = list.push_front('c');
        let b = list.push_front('b');
        assert_eq!(list.remove(c), Some('c'));
        assert_eq!(list.remove(b), Some('b'));

        let c = list.push_front('c');
        let b = list.push_front('b');
        let a = list.push_front('a');
        assert_eq!(list.remove(c), Some('c'));
        assert_eq!(list.remove(b), Some('b'));
        assert_eq!(list.remove(a), Some('a'));
    }
}
