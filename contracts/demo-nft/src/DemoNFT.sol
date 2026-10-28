// SPDX-License-Identifier: MIT
pragma solidity 0.8.38;

import {ERC721URIStorage} from "@openzeppelin/contracts/token/ERC721/extensions/ERC721URIStorage.sol";
import {ERC721} from "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

/// Disposable local-network collectibles. Ownership follows ordinary ERC-721 rules.
contract DemoNFT is ERC721URIStorage, Ownable {
    constructor() ERC721("Void Market Demo", "VOID") Ownable(msg.sender) {}

    function mint(address recipient, uint256 tokenId, string calldata metadataUri) external onlyOwner {
        _safeMint(recipient, tokenId);
        _setTokenURI(tokenId, metadataUri);
    }
}
