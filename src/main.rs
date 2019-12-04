#[macro_use]
extern crate log;

use crate::executor::{run_async, Executor};
use specs::prelude::{DispatcherBuilder, World, WorldExt};

mod executor;

fn main() {
    let mut world = World::new();
    let dispatcher_builder =
        DispatcherBuilder::new().with(Executor::<u32>::new(), "u32_executor", &[]);

    let mut dispatcher = dispatcher_builder.build();
    dispatcher.setup(&mut world);

    // Run once to ensure reader ids are instantiated
    dispatcher.dispatch(&world);

    world.exec(|ec| run_async(ec, async { 8u32 }));

    loop {
        dispatcher.dispatch(&world);
    }
}
