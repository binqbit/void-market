// SPDX-License-Identifier: MIT
pragma solidity 0.8.38;

import {DemoNFT} from "../src/DemoNFT.sol";

interface Vm {
    function prank(address sender) external;
    function expectRevert() external;
}

contract DemoNFTTest {
    Vm private constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function testCustodyAndDeliveryPreserveTokenAndMetadata() public {
        DemoNFT collection = new DemoNFT();
        address seller = address(0xA11CE);
        address custodian = address(0xCAFE);
        address buyer = address(0xB0B);
        collection.mint(seller, 42, "https://demo.invalid/42.json");
        require(collection.ownerOf(42) == seller, "minted to wrong owner");

        vm.prank(seller);
        collection.safeTransferFrom(seller, custodian, 42);
        require(collection.ownerOf(42) == custodian, "custody not established");
        require(collection.getApproved(42) == address(0), "unexpected broad authority");

        vm.prank(custodian);
        collection.safeTransferFrom(custodian, buyer, 42);
        require(collection.ownerOf(42) == buyer, "delivery not established");
        require(keccak256(bytes(collection.tokenURI(42))) == keccak256("https://demo.invalid/42.json"));
    }

    function testOnlyTheMinterCanCreateCollectibles() public {
        DemoNFT collection = new DemoNFT();
        vm.expectRevert();
        vm.prank(address(0xB0B));
        collection.mint(address(0xB0B), 42, "https://demo.invalid/42.json");
    }

    function testAThirdPartyCannotTakeCustodyOrDeliverTheToken() public {
        DemoNFT collection = new DemoNFT();
        collection.mint(address(0xA11CE), 42, "https://demo.invalid/42.json");
        vm.expectRevert();
        vm.prank(address(0xB0B));
        collection.safeTransferFrom(address(0xA11CE), address(0xB0B), 42);
    }
}
