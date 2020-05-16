use dbus_tokio::connection;
use futures::future;
use tokio::time::delay_for;
use dbus::channel::{MatchingReceiver, Sender};
use dbus::message::MatchRule;
use dbus_crossroads::{Crossroads, Context};
use std::time::Duration;
use std::sync::Arc;

// This is our "Hello" object that we are going to store inside the crossroads instance.
struct Hello { called_count: u32 }

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {

    // Connect to the D-Bus session bus (this is blocking, unfortunately).
    let (resource, c) = connection::new_session_sync()?;

    // The resource is a task that should be spawned onto a tokio compatible
    // reactor ASAP. If the resource ever finishes, you lost connection to D-Bus.
    tokio::spawn(async {
        let err = resource.await;
        panic!("Lost connection to D-Bus: {}", err);
    });

    // Let's request a name on the bus, so that clients can find us.
    c.request_name("com.example.dbustest", false, true, false).await?;

    // Create a new crossroads instance.
    // The instance is configured so that introspection and properties interfaces
    // are added by default on object path additions.
    let mut cr = Crossroads::new();

    // Enable async support for the crossroads instance.
    let c = Arc::new(c);
    let cclone: Arc<dyn Sender + Send + Sync + 'static> = c.clone() as Arc<dyn Sender + Send + Sync + 'static>;
    cr.set_async_support(Some((cclone, Box::new(|x| { tokio::spawn(x); }))));

    // Let's build a new interface, which can be used for "Hello" objects.
    let iface_token = cr.register("com.example.dbustest", |b| {
        // This row is just for introspection: It advertises that we can send a
        // HelloHappened signal. We use the single-tuple to say that we have one single argument,
        // named "sender" of type "String".
        b.signal::<(String,), _>("HelloHappened", ("sender",));
        // Let's add a method to the interface. We have the method name, followed by
        // names of input and output arguments (used for introspection). The closure then controls
        // the types of these arguments. The last argument to the closure is a tuple of the input arguments.
        b.method_with_cr_async("Hello", ("name",), ("reply",), |mut ctx, cr, (name,): (String,)| {
            let hello: &mut Hello = cr.data_mut(ctx.path()).unwrap(); // ok_or_else(|| MethodErr::no_path(ctx.path()))?;
            // And here's what happens when the method is called.
            println!("Incoming hello call from {}!", name);
            hello.called_count += 1;
            let s = format!("Hello {}! This API has been used {} times.", name, hello.called_count);
            cr.spawn_method(ctx, async move {
                // Let's wait half a second just to show off how async we are.
                delay_for(Duration::from_millis(500)).await;
                // The ctx parameter can be used to conveniently send extra messages.
                // let signal_msg = ctx.make_signal("HelloHappened", (name,));
                // ctx.push_msg(signal_msg);
                // And the return value is a tuple of the output arguments.
                Ok((s,))
            })
        });
    });

    // Let's add the "/hello" path, which implements the com.example.dbustest interface,
    // to the crossroads instance.
    cr.insert("/hello", &[iface_token], Hello { called_count: 0});

    // We add the Crossroads instance to the connection so that incoming method calls will be handled.
    c.start_receive(MatchRule::new_method_call(), Box::new(move |msg, conn| {
        cr.handle_message(msg, conn).unwrap();
        true
    }));

    // Run forever.
    future::pending::<()>().await;
    unreachable!()
}
