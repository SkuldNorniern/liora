use super::Heap;
use crate::runtime::{GeneratorState, PromiseRecord, PromiseState};

impl Heap {
    pub fn alloc_generator(&mut self, state: GeneratorState) -> usize {
        let id = self.generator_states.len();
        self.generator_states.push(state);
        id
    }

    pub fn get_generator(&self, id: usize) -> Option<&GeneratorState> {
        self.generator_states.get(id)
    }

    pub fn get_generator_mut(&mut self, id: usize) -> Option<&mut GeneratorState> {
        self.generator_states.get_mut(id)
    }

    pub fn alloc_promise(&mut self, state: PromiseState) -> usize {
        let id = self.promises.len();
        self.promises.push(PromiseRecord {
            state,
            callbacks: Vec::new(),
        });
        id
    }

    pub fn get_promise(&self, id: usize) -> Option<&PromiseRecord> {
        self.promises.get(id)
    }

    pub fn get_promise_mut(&mut self, id: usize) -> Option<&mut PromiseRecord> {
        self.promises.get_mut(id)
    }
}
