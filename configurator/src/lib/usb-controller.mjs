import S from "@surplus/s";

const VENDOR_ID = 0x1337;

const getDevice = ({ vendorId }) => {
	console.log("requesting device...");
};

export function device({ enable }) {
	const dev = S.value(undefined);

	return S(() => {
		if (dev()) {
			S.cleanup(() => {
				console.log("destroying device...");
				dev().close();
			});
		}

		if (dev() && enable()) {
			console.log("product name:", dev().productName);
			console.log("manufacturer name:", dev().manufacturerName);
			return dev();
		} else if (enable()) {
			console.log("requesting device with vendor ID", VENDOR_ID);
			navigator.usb
				.requestDevice({
					filters: [{ vendorId: VENDOR_ID }],
				})
				.then(dev);
		}
	});
}
