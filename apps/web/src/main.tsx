import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter, Link, Route, Routes } from "react-router-dom";
import { SessionProvider } from "./auth/session";
import { Layout } from "./components/layout";
import { HomePage } from "./pages/home";
import { MarketPage } from "./pages/market";
import { DocsPage } from "./pages/docs";
import { LoginPage } from "./pages/login";
import { AccountPage } from "./pages/account";
import { PortfolioPage } from "./pages/portfolio";
import "./styles.css";
import { MessagesProvider } from "./messages/provider";
import { MessagesPage } from "./pages/messages";
import { AuctionPage } from "./pages/auction";
import { DemoProvider } from "./demo/config";
import { DemoPage } from "./pages/demo";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <BrowserRouter>
      <DemoProvider>
        <SessionProvider>
          <MessagesProvider>
            <Routes>
              <Route element={<Layout />}>
                <Route index element={<HomePage />} />
                <Route path="market" element={<MarketPage />} />
                <Route path="market/:id" element={<AuctionPage />} />
                <Route path="portfolio" element={<PortfolioPage />} />
                <Route path="messages" element={<MessagesPage />} />
                <Route path="docs" element={<DocsPage />} />
                <Route path="login" element={<LoginPage />} />
                <Route path="account" element={<AccountPage />} />
                <Route path="demo" element={<DemoPage />} />
                <Route
                  path="*"
                  element={
                    <section className="empty-state page-width">
                      <h1>Page not found</h1>
                      <p>
                        This address does not point to a page in Void Market.
                      </p>
                      <Link className="button primary" to="/">
                        Back to home
                      </Link>
                    </section>
                  }
                />
              </Route>
            </Routes>
          </MessagesProvider>
        </SessionProvider>
      </DemoProvider>
    </BrowserRouter>
  </React.StrictMode>,
);
