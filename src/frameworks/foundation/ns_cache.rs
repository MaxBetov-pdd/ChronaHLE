/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSCache`.

use super::NSUInteger;
use crate::mem::{guest_size_of, MutPtr, Ptr};
use crate::objc::{
    id, msg, msg_send, nil, objc_classes, objc_destroyWeak, objc_initWeak, objc_loadWeak,
    objc_storeWeak, release, retain, ClassExports, HostObject, NSZonePtr,
};
use crate::Environment;
use std::collections::HashMap;

type Hash = NSUInteger;

struct CacheEntry {
    key: id,
    value: id,
    cost: NSUInteger,
    sequence: u64,
}

struct NSCacheHostObject {
    entries: HashMap<Hash, Vec<CacheEntry>>,
    count: NSUInteger,
    total_cost: NSUInteger,
    count_limit: NSUInteger,
    total_cost_limit: NSUInteger,
    next_sequence: u64,
    delegate_slot: MutPtr<id>,
    name: id,
    evicts_discarded_content: bool,
}
impl HostObject for NSCacheHostObject {}

impl Default for NSCacheHostObject {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            count: 0,
            total_cost: 0,
            count_limit: 0,
            total_cost_limit: 0,
            next_sequence: 0,
            delegate_slot: Ptr::null(),
            name: nil,
            evicts_discarded_content: true,
        }
    }
}

impl NSCacheHostObject {
    fn lookup(&self, env: &mut Environment, key: id) -> id {
        let hash: Hash = msg![env; key hash];
        let Some(collisions) = self.entries.get(&hash) else {
            return nil;
        };
        collisions
            .iter()
            .find(|entry| entry.key == key || msg![env; (entry.key) isEqual:key])
            .map_or(nil, |entry| entry.value)
    }

    fn insert(&mut self, env: &mut Environment, key: id, value: id, cost: NSUInteger) {
        let hash: Hash = msg![env; key hash];
        let key = retain(env, key);
        let value = retain(env, value);
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);

        let collisions = self.entries.entry(hash).or_default();
        if let Some(entry) = collisions
            .iter_mut()
            .find(|entry| entry.key == key || msg![env; (entry.key) isEqual:key])
        {
            release(env, key);
            release(env, entry.value);
            self.total_cost = self.total_cost.saturating_sub(entry.cost);
            self.total_cost = self.total_cost.saturating_add(cost);
            entry.value = value;
            entry.cost = cost;
            entry.sequence = sequence;
            return;
        }

        collisions.push(CacheEntry {
            key,
            value,
            cost,
            sequence,
        });
        self.count = self.count.saturating_add(1);
        self.total_cost = self.total_cost.saturating_add(cost);
    }

    fn remove(&mut self, env: &mut Environment, key: id) {
        let hash: Hash = msg![env; key hash];
        let Some(collisions) = self.entries.get_mut(&hash) else {
            return;
        };
        let Some(index) = collisions
            .iter()
            .position(|entry| entry.key == key || msg![env; (entry.key) isEqual:key])
        else {
            return;
        };
        let entry = collisions.remove(index);
        self.count -= 1;
        self.total_cost = self.total_cost.saturating_sub(entry.cost);
        release(env, entry.key);
        release(env, entry.value);
        if collisions.is_empty() {
            self.entries.remove(&hash);
        }
    }

    fn take_oldest(&mut self) -> Option<CacheEntry> {
        let (hash, index) = self
            .entries
            .iter()
            .flat_map(|(hash, entries)| {
                entries
                    .iter()
                    .enumerate()
                    .map(move |(index, entry)| (*hash, index, entry.sequence))
            })
            .min_by_key(|(_, _, sequence)| *sequence)
            .map(|(hash, index, _)| (hash, index))?;
        let collisions = self.entries.get_mut(&hash).unwrap();
        let entry = collisions.remove(index);
        self.count -= 1;
        self.total_cost = self.total_cost.saturating_sub(entry.cost);
        if collisions.is_empty() {
            self.entries.remove(&hash);
        }
        Some(entry)
    }

    fn over_limit(&self) -> bool {
        (self.count_limit != 0 && self.count > self.count_limit)
            || (self.total_cost_limit != 0 && self.total_cost > self.total_cost_limit)
    }

    fn release_all(&mut self, env: &mut Environment) {
        for entry in self.entries.values().flatten() {
            release(env, entry.key);
            release(env, entry.value);
        }
        self.entries.clear();
        self.count = 0;
        self.total_cost = 0;
    }
}

fn notify_and_release_evicted(env: &mut Environment, cache: id, entry: CacheEntry, delegate: id) {
    if delegate != nil {
        if let Some(selector) = env.objc.lookup_selector("cache:willEvictObject:") {
            if env.objc.object_has_method(&env.mem, delegate, selector) {
                let _: () = msg_send(env, (delegate, selector, cache, entry.value));
            }
        }
    }
    release(env, entry.key);
    release(env, entry.value);
}

fn enforce_limits(env: &mut Environment, cache: id, host: &mut NSCacheHostObject) {
    let delegate = objc_loadWeak(env, host.delegate_slot);
    while host.over_limit() {
        let entry = host.take_oldest().unwrap();
        notify_and_release_evicted(env, cache, entry, delegate);
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSCache: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let delegate_slot: MutPtr<id> = env.mem.alloc(guest_size_of::<id>()).cast();
    objc_initWeak(env, delegate_slot, nil);
    let host_object = NSCacheHostObject {
        delegate_slot,
        ..Default::default()
    };
    env.objc.alloc_object(this, Box::new(host_object), &mut env.mem)
}

- (id)init {
    this
}

- (id)objectForKey:(id)key {
    assert_ne!(key, nil);
    let host: NSCacheHostObject = std::mem::take(env.objc.borrow_mut(this));
    let value = host.lookup(env, key);
    *env.objc.borrow_mut(this) = host;
    value
}

- (())setObject:(id)object forKey:(id)key {
    msg![env; this setObject:object forKey:key cost:(0u32)]
}

- (())setObject:(id)object forKey:(id)key cost:(NSUInteger)cost {
    assert_ne!(object, nil);
    assert_ne!(key, nil);
    let mut host: NSCacheHostObject = std::mem::take(env.objc.borrow_mut(this));
    host.insert(env, key, object, cost);
    enforce_limits(env, this, &mut host);
    *env.objc.borrow_mut(this) = host;
}

- (())removeObjectForKey:(id)key {
    assert_ne!(key, nil);
    let mut host: NSCacheHostObject = std::mem::take(env.objc.borrow_mut(this));
    host.remove(env, key);
    *env.objc.borrow_mut(this) = host;
}

- (())removeAllObjects {
    let mut host: NSCacheHostObject = std::mem::take(env.objc.borrow_mut(this));
    host.release_all(env);
    *env.objc.borrow_mut(this) = host;
}

- (NSUInteger)countLimit {
    env.objc.borrow::<NSCacheHostObject>(this).count_limit
}

- (())setCountLimit:(NSUInteger)limit {
    let mut host: NSCacheHostObject = std::mem::take(env.objc.borrow_mut(this));
    host.count_limit = limit;
    enforce_limits(env, this, &mut host);
    *env.objc.borrow_mut(this) = host;
}

- (NSUInteger)totalCostLimit {
    env.objc.borrow::<NSCacheHostObject>(this).total_cost_limit
}

- (())setTotalCostLimit:(NSUInteger)limit {
    let mut host: NSCacheHostObject = std::mem::take(env.objc.borrow_mut(this));
    host.total_cost_limit = limit;
    enforce_limits(env, this, &mut host);
    *env.objc.borrow_mut(this) = host;
}

- (id)delegate {
    let slot = env.objc.borrow::<NSCacheHostObject>(this).delegate_slot;
    objc_loadWeak(env, slot)
}

- (())setDelegate:(id)delegate {
    let slot = env.objc.borrow::<NSCacheHostObject>(this).delegate_slot;
    objc_storeWeak(env, slot, delegate);
}

- (id)name {
    env.objc.borrow::<NSCacheHostObject>(this).name
}

- (())setName:(id)name {
    let name: id = msg![env; name copy];
    let old_name = env.objc.borrow::<NSCacheHostObject>(this).name;
    env.objc.borrow_mut::<NSCacheHostObject>(this).name = name;
    release(env, old_name);
}

- (bool)evictsObjectsWithDiscardedContent {
    env.objc
        .borrow::<NSCacheHostObject>(this)
        .evicts_discarded_content
}

- (())setEvictsObjectsWithDiscardedContent:(bool)evicts {
    env.objc
        .borrow_mut::<NSCacheHostObject>(this)
        .evicts_discarded_content = evicts;
}

- (())dealloc {
    let mut host: NSCacheHostObject = std::mem::take(env.objc.borrow_mut(this));
    host.release_all(env);
    release(env, host.name);
    objc_destroyWeak(env, host.delegate_slot);
    env.mem.free(host.delegate_slot.cast());
    env.objc.dealloc_object(this, &mut env.mem)
}

@end

};
