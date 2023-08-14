// Run with: node demo.js
globalThis.WebSocket = require("websocket").w3cwebsocket;

const {
    PrivateKey,
    Address,
    RpcClient,
    Encoding,
    NetworkType,
    UtxoEntries,
    kaspaToSompi,
    createTransactions,
    init_console_panic_hook
} = require('./kaspa/kaspa_wasm');

init_console_panic_hook();

async function runDemo() {
    let args = process.argv.slice(2);
    let destination = args.shift() || "kaspatest:qqkl0ct62rv6dz74pff2kx5sfyasl4z28uekevau23g877r5gt6userwyrmtt";
    console.log("using destination address:", destination);
    
    // ---
    // network type
    let network = NetworkType.Testnet;
    // RPC encoding
    let encoding = Encoding.Borsh;
    // ---

    // From BIP0340
    const sk = new PrivateKey('b7e151628aed2a6abf7158809cf4f3c762e7160f38b4da56a784d9045190cfef');

    const kaspaAddress = sk.toKeypair().toAddress(network).toString();
    // Full kaspa address: kaspa:qr0lr4ml9fn3chekrqmjdkergxl93l4wrk3dankcgvjq776s9wn9jkdskewva
    console.info(`Full kaspa address: ${kaspaAddress}`);

    const address = new Address(kaspaAddress);
    console.info(address);
    console.info(sk.toKeypair().xOnlyPublicKey); // dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659
    console.info(sk.toKeypair().publicKey);      // 02dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659

    let rpcUrl = RpcClient.parseUrl("127.0.0.1", encoding, network);
    const rpc = new RpcClient(encoding, rpcUrl, network);
    console.log(`Connecting to ${rpc.url}`);

    await rpc.connect();

    let entries = await rpc.getUtxosByAddresses([address]);
    
    if (!entries.length) {
        console.error("No UTXOs found for address");
    } else {
        console.info(entries);
        
        // a very basic JS-driven utxo entry sort
        entries.sort((a, b) => a.utxoEntry.amount > b.utxoEntry.amount || -(a.utxoEntry.amount < b.utxoEntry.amount));

        let { transactions, summary } = await createTransactions({
            entries, 
            outputs : [[destination, kaspaToSompi(0.2)]],
            priorityFee: 0,
            changeAddress: address,
        });

        console.log("summary:", summary);

        for (let pending of transactions) {
            console.log("pending transaction:", pending);
            console.log("signing tx with secret key:",sk.toString());
            await pending.sign([sk]);
            console.log("submitting pending tx to RPC ...")
            let txid = await pending.submit(rpc);
            console.log("node responded with txid:", txid);
        }
    }

    rpc.disconnect();
}

runDemo();