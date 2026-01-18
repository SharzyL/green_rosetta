import type { ReactElement } from "react";
import "./App.css";
import "./OutsideNow.css";

const OutsideNow = (): ReactElement => {
	return (
		<div className="container">
			<div className="player-container">
				<h1 className="app-title">Outside Now</h1>

				<div className="player-card outsideNowCard">
					<div className="outsideNowCard__inner">
						<div className="outsideNowCard__headline">404</div>
						<div className="outsideNowCard__text">
							This page is not in the current manifest.
						</div>
						<a className="outsideNowCard__link" href="/">
							Back to the stream
						</a>
					</div>
				</div>
			</div>
		</div>
	);
};

export default OutsideNow;
