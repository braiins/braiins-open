// Copyright (C) 2020  Braiins Systems s.r.o.
//
// This file is part of Braiins Open-Source Initiative (BOSI).
//
// BOSI is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// Please, keep in mind that we may also license BOSI or any part thereof
// under a proprietary license. For more information on the terms and conditions
// of such proprietary license or if you have any other questions, please
// contact us at opensource@braiins.com.

use ii_unvariant::{handler, GetId, Handler, Id};

mod common;
use common::*;

#[test]
fn dyn_sync_handler() {
    struct MyHandler;

    #[handler(Frame)]
    impl MyHandler {
        // Here the type that this handler method handles is gleaned from the input type ...
        fn handle_foo(&mut self, foo: Foo) -> Result<u32, u32> {
            Ok(foo.into())
        }

        // ... but you can also use an explicit annotation:
        #[handle(Bar)]
        fn handle_bar(&mut self, bar: Bar) -> Result<u32, u32> {
            Ok(bar.into())
        }

        #[handle(_)]
        fn handle_unknown(&mut self, frame: Frame) -> Result<u32, u32> {
            Err(frame.get_id())
        }
    }

    type MyHandlerDyn = dyn Handler<Frame, Result<u32, u32>>;

    // static handler - generated name & alias
    let mut static_handler = MyHandler;

    let res = static_handler.handle(Frame::new_foo(9001));
    assert_eq!(res, Ok(9001 + Foo::ID));
    let res = static_handler.handle(Frame::new_bar(true));
    assert_eq!(res, Ok(2 + Bar::ID));
    let res = static_handler.handle(Frame::new_unknown());
    assert_eq!(res, Err(0xff));

    // Verify that we can make a trait object:
    // dynamic polymorphic handler
    let mut dyn_handler: Box<MyHandlerDyn> = Box::new(MyHandler);

    let res = dyn_handler.handle(Frame::new_foo(9001));
    assert_eq!(res, Ok(9001 + Foo::ID));
    let res = dyn_handler.handle(Frame::new_bar(true));
    assert_eq!(res, Ok(2 + Bar::ID));
    let res = dyn_handler.handle(Frame::new_unknown());
    assert_eq!(res, Err(0xff));
}

#[test]
fn sync_try_handler() {
    struct MyHandler;

    #[handler(try Frame)]
    impl MyHandler {
        fn handle_foo(&mut self, res: TryFoo) -> Result<u32, u32> {
            Ok(res.into())
        }

        #[handle(TryBar)]
        fn handle_bar(&mut self, res: TryBar) -> Result<u32, u32> {
            Ok(res.into())
        }

        #[handle(_)]
        fn handle_unknown(&mut self, frame: Result<Frame, u32>) -> Result<u32, u32> {
            Err(frame?.get_id())
        }
    }

    // static handler
    let mut handler = MyHandler;

    let res = handler.handle(Frame::new_foo(9001));
    assert_eq!(res, Ok(9001 + TryFoo::ID));
    let res = handler.handle(Frame::new_bar(true));
    assert_eq!(res, Ok(2 + TryBar::ID));
    let res = handler.handle(Frame::new_foo_bad());
    assert_eq!(res, Err(TryFoo::ID));
    let res = handler.handle(Frame::new_bar_bad());
    assert_eq!(res, Err(TryBar::ID));
    let res = handler.handle(Frame::new_unknown());
    assert_eq!(res, Err(0xff));
}

#[tokio::test]
async fn async_handler() {
    struct MyHandler;

    #[handler(async Frame)]
    impl MyHandler {
        async fn handle_foo(&mut self, foo: Foo) -> Result<u32, u32> {
            Ok(foo.into())
        }

        fn handle_bar(&mut self, bar: Bar) -> Result<u32, u32> {
            Ok(bar.into())
        }

        #[handle(_)]
        fn handle_unknown(&mut self, frame: Frame) -> Result<u32, u32> {
            Err(frame.get_id())
        }

        // This shouldn't compile / should output sensible error:
        // TODO: how to test this stuff? `trybuild`?
        // Also possible through doc tests
        // #[handle(_)]
        // fn handle_unknown2(&mut self, frame: Frame) -> Result<u32, u32> {
        //     Err(id)
        // }
    }

    // static handler
    let mut static_handler = MyHandler;

    let res = static_handler.handle(Frame::new_foo(9001)).await;
    assert_eq!(res, Ok(9001 + Foo::ID));

    let res = static_handler.handle(Frame::new_bar(true)).await;
    assert_eq!(res, Ok(2 + Bar::ID));

    let res = static_handler.handle(Frame::new_unknown()).await;
    assert_eq!(res, Err(0xff));

    // dynamic polymorphic handler
    let mut dyn_handler = MyHandler.into_handler();

    let res = dyn_handler.handle(Frame::new_foo(9001)).await;
    assert_eq!(res, Ok(9001 + Foo::ID));

    let res = dyn_handler.handle(Frame::new_bar(true)).await;
    assert_eq!(res, Ok(2 + Bar::ID));

    let res = dyn_handler.handle(Frame::new_unknown()).await;
    assert_eq!(res, Err(0xff));
}

#[tokio::test]
async fn async_handler_suffix() {
    struct MyHandler;

    #[handler(async Frame suffix _mysuffix)]
    impl MyHandler {
        async fn handle_foo(&mut self, foo: Foo) -> Result<u32, u32> {
            Ok(foo.into())
        }

        fn handle_bar(&mut self, bar: Bar) -> Result<u32, u32> {
            Ok(bar.into())
        }

        #[handle(_)]
        fn handle_unknown(&mut self, frame: Frame) -> Result<u32, u32> {
            Err(frame.get_id())
        }
    }

    // static handler
    let mut static_handler = MyHandler;

    let res = static_handler.handle_mysuffix(Frame::new_foo(9001)).await;
    assert_eq!(res, Ok(9001 + Foo::ID));

    let res = static_handler.handle_mysuffix(Frame::new_bar(true)).await;
    assert_eq!(res, Ok(2 + Bar::ID));

    let res = static_handler.handle_mysuffix(Frame::new_unknown()).await;
    assert_eq!(res, Err(0xff));

    // dynamic polymorphic handler
    let mut dyn_handler = MyHandler.into_handler_mysuffix();

    let res = dyn_handler.handle(Frame::new_foo(9001)).await;
    assert_eq!(res, Ok(9001 + Foo::ID));

    let res = dyn_handler.handle(Frame::new_bar(true)).await;
    assert_eq!(res, Ok(2 + Bar::ID));

    let res = dyn_handler.handle(Frame::new_unknown()).await;
    assert_eq!(res, Err(0xff));
}

#[tokio::test]
async fn async_try_handler() {
    struct MyHandler;

    #[handler(async try Frame)]
    impl MyHandler {
        async fn handle_foo(&mut self, res: TryFoo) -> Result<u32, u32> {
            Ok(res.into())
        }

        fn handle_bar(&mut self, res: TryBar) -> Result<u32, u32> {
            Ok(res.into())
        }

        #[handle(_)]
        fn handle_unknown(&mut self, frame: Result<Frame, u32>) -> Result<u32, u32> {
            Err(frame?.get_id())
        }
    }

    // static handler
    let mut static_handler = MyHandler;

    let res = static_handler.handle(Frame::new_foo(9001)).await;
    assert_eq!(res, Ok(9001 + Foo::ID));

    let res = static_handler.handle(Frame::new_bar(true)).await;
    assert_eq!(res, Ok(2 + Bar::ID));

    let res = static_handler.handle(Frame::new_unknown()).await;
    assert_eq!(res, Err(0xff));

    // dynamic polymorphic handler
    let mut dyn_handler = MyHandler.into_handler();

    let res = dyn_handler.handle(Frame::new_foo(9001)).await;
    assert_eq!(res, Ok(9001 + Foo::ID));

    let res = dyn_handler.handle(Frame::new_bar(true)).await;
    assert_eq!(res, Ok(2 + Bar::ID));

    let res = dyn_handler.handle(Frame::new_unknown()).await;
    assert_eq!(res, Err(0xff));
}

// --- ref handlers ---

// TODO: sync ref handler ?

#[test]
fn sync_try_ref_handler() {
    struct MyHandler;

    #[handler(try &'a StrFrame<'b> trait TryStrFrameHandler)]
    impl MyHandler {
        fn handle_foo(&mut self, _res: TryStrFoo) -> Result<(), bool> {
            Ok(())
        }

        #[handle(TryStrBar)]
        fn handle_bar(&mut self, _res: TryStrBar) -> Result<(), bool> {
            Ok(())
        }

        #[handle(_)]
        fn handle_unknown<'a, 'b>(
            &mut self,
            frame: Result<&'a StrFrame<'a>, TryStrFrameError>,
        ) -> Result<(), bool> {
            Err(frame?.1)
        }
    }

    let mut handler = MyHandler;

    let foo = "foo".to_string();
    let bar = "bar".to_string();
    let xxx = "xxx".to_string();
    let frame_foo = StrFrame(&foo, true);
    let frame_bar = StrFrame(&bar, true);
    let frame_foo_bad = StrFrame(&foo, false);
    let frame_bar_bad = StrFrame(&bar, false);
    let frame_unknown = StrFrame(&xxx, true);

    let res = handler.handle(&frame_foo);
    assert_eq!(res, Ok(()));

    let res = handler.handle(&frame_bar);
    assert_eq!(res, Ok(()));

    let res = handler.handle(&frame_foo_bad);
    assert_eq!(res, Err(false));

    let res = handler.handle(&frame_bar_bad);
    assert_eq!(res, Err(false));

    let res = handler.handle(&frame_unknown);
    assert_eq!(res, Err(true));
}

#[tokio::test]
async fn async_ref_handler() {
    struct MyHandler;

    #[handler(async &'a StrFrame<'b>)]
    impl MyHandler {
        async fn handle_foo(&mut self, _foo: StrFoo) -> Result<u32, ()> {
            Ok(1)
        }

        fn handle_bar(&mut self, _bar: StrBar) -> Result<u32, ()> {
            Ok(2)
        }

        #[handle(_)]
        fn handle_unknown<'a, 'b>(&mut self, _frame: &'a StrFrame<'b>) -> Result<u32, ()> {
            Err(())
        }
    }

    let foo = "foo".to_string();
    let bar = "bar".to_string();
    let xxx = "xxx".to_string();
    let frame_foo = StrFrame(&foo, true);
    let frame_bar = StrFrame(&bar, true);
    let frame_unknown = StrFrame(&xxx, true);

    let mut handler = MyHandler;

    let res = handler.handle(&frame_foo).await;
    assert_eq!(res, Ok(1));

    let res = handler.handle(&frame_bar).await;
    assert_eq!(res, Ok(2));

    let res = handler.handle(&frame_unknown).await;
    assert_eq!(res, Err(()));

    // dynamic handler not supported in async + ref variant
}

#[tokio::test]
async fn async_try_ref_handler() {
    struct MyHandler;

    #[handler(async try &'a StrFrame<'b>)]
    impl MyHandler {
        async fn handle_foo(&mut self, _res: TryStrFoo) -> Result<(), bool> {
            Ok(())
        }

        fn handle_bar(&mut self, _res: TryStrBar) -> Result<(), bool> {
            Ok(())
        }

        #[handle(_)]
        async fn handle_unknown<'a, 'b>(
            &mut self,
            frame: Result<&'a StrFrame<'b>, TryStrFrameError>,
        ) -> Result<(), bool> {
            Err(frame?.1)
        }
    }

    let mut handler = MyHandler;

    let foo = "foo".to_string();
    let bar = "bar".to_string();
    let xxx = "xxx".to_string();
    let frame_foo = StrFrame(&foo, true);
    let frame_foo_bad = StrFrame(&foo, false);
    let frame_bar = StrFrame(&bar, true);
    let frame_unknown = StrFrame(&xxx, true);

    let res = handler.handle(&frame_foo).await;
    assert_eq!(res, Ok(()));

    let res = handler.handle(&frame_foo_bad).await;
    assert_eq!(res, Err(false));

    let res = handler.handle(&frame_bar).await;
    assert_eq!(res, Ok(()));

    let res = handler.handle(&frame_unknown).await;
    assert_eq!(res, Err(true));

    // dynamic handler not supported in async + ref variant
}
